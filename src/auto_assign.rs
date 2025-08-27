use std::collections::HashMap;

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Expr, Ident};

use crate::helper::AutoAssignMacro;

/// macro assembler for #[auto_assign(...)]
pub fn auto_assign_macro_factory(
  struct_name: &Ident,
  struct_fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> TokenStream2 {
  let (field_idents, field_types) = struct_fields.iter().fold(
    (Vec::new(), Vec::new()),
    |(mut idents, mut types), f| {
      idents.push(f.ident.as_ref().unwrap().clone());
      types.push(f.ty.clone());
      (idents, types)
    },
  );
  let field_override_handler_macro_ts =
    generate_fields_overrider_macro_ts(&field_idents, &field_types);

  let input_validator_macro_ts =
    generate_input_validation_macro_ts(struct_name, &field_idents);

  let helper_trait_ts = get_type_handler_trait_ts();
  let builder_macro_name = InternMacro::Entrypoint(struct_name).to_ident();

  let factory = quote! {
    #[allow(unused_macros)]
    #input_validator_macro_ts

    #[allow(unused_macros)]
    #field_override_handler_macro_ts

    #[allow(unused_macros)]
    macro_rules! #builder_macro_name {
      ($self:expr, $phantom_state:expr, $($pairs:tt)*) => {
        {
        #helper_trait_ts
          #struct_name {
            #(#field_idents: default_or_override!($self, #field_idents, [ $($pairs)*]),)*
            _state: $phantom_state
          }
        }
      }
    }
  };
  factory
}

pub fn process_auto_assign(
  func_ident: &Ident,
  struct_name: &Ident,
  phantom_state_field: TokenStream2,
  assign_attr: AutoAssignMacro,
) -> crate::Result<TokenStream2> {
  let mut validator_macro_name =
    InternMacro::InputValidation(struct_name, func_ident.span()).to_ident();
  validator_macro_name.set_span(func_ident.span());
  let usr_assignments = validate_and_get_usr_input(&assign_attr)?;
  let validation_calls: Vec<_> = usr_assignments
    .keys()
    .map(|&k| quote! {#validator_macro_name!(#k);})
    .collect();
  let generated_code = quote! {
      const _: () = {#( #validation_calls )*};
  };

  let kv_arms: Vec<_> = usr_assignments
    .into_iter()
    .map(|(field_to_update, expression_to_assign)| {
      quote! {#field_to_update = #expression_to_assign
      }
    })
    .collect();

  let mut builder_macro_name = InternMacro::Entrypoint(struct_name).to_ident();
  builder_macro_name.set_span(func_ident.span());
  Ok(quote! {
    #generated_code
    #builder_macro_name!(self, #phantom_state_field, #(#kv_arms),* )
  })
}

/// Unique per struct id generator for the internal factory macros.
enum InternMacro<'a> {
  Entrypoint(&'a Ident),
  InputValidation(&'a Ident, Span),
}
impl InternMacro<'_> {
  const fn macro_prefix() -> &'static str { "__state_shift_auto_assign" }

  fn to_ident(&self) -> Ident {
    match self {
      InternMacro::Entrypoint(struct_name) => Ident::new(
        &format!(
          "{}_{}",
          InternMacro::macro_prefix(),
          struct_name.to_string().to_lowercase()
        ),
        struct_name.span(),
      ),
      InternMacro::InputValidation(struct_name, span) => Ident::new(
        &format!(
          "{}_validate_input_keys",
          InternMacro::to_ident(&InternMacro::Entrypoint(struct_name))
        ),
        *span,
      ),
    }
  }
}

fn get_type_handler_trait_ts() -> TokenStream2 {
  quote! {
    trait OptionWrapper<T> {
      fn wrap(self) -> Option<T>;
    }// caller is allowed to pass in non-wrapped values  -> auto-wrapping as-a-service
    impl<T> OptionWrapper<T> for T {
      fn wrap(self) -> Option<T> {
          Some(self)
      }
    }// caller input is already wrapped, this is the identity function
    impl<T> OptionWrapper<T> for Option<T> {
      fn wrap(self) -> Option<T> {
          self
      }
    }
  }
}
fn generate_fields_overrider_macro_ts(
  f_idents: &[Ident],
  f_types: &[syn::Type],
) -> TokenStream2 {
  // By default, a struct field is set to self.<field_id>.
  // The user can override this by specifying his `key = key_alias` pairs in the attribute list #[auto_assign(...)]
  // Here we construct the macro that is responsible for setting default values and their overrides
  let override_handlers = f_idents
    .iter()
    .zip(f_types.iter())
    .map(|(id, ty)| wrap_option_type(id, ty));

  quote! {
    macro_rules! default_or_override {
      // base case
      ($self:expr, $field:ident,[]) => {$self.$field};

      #(#override_handlers)*

      // current K/V was not matched, it might be further down the list -> drop this pair and recurse
      ($self:expr, $field:ident, [$other:ident = $val:expr, $($rest:tt)*]) => {
        default_or_override!($self, $field, [$($rest)*])
      };

      // we reached the final item of the input list and didn't find any matches -> exhaust the list and recurse
      ($self:expr, $field:ident, [$other:ident = $val:expr]) => {default_or_override!($self, $field,[])};
     }
  }
}

fn generate_input_validation_macro_ts(
  struct_name: &Ident,
  f_idents: &[Ident],
) -> TokenStream2 {
  let valid_field_arms = f_idents.iter().map(|ident| {
    quote! { (#ident) => {}; } // the input key is valid -> we do nothing
  });
  let valid_fields_list_str = f_idents
    .iter()
    .map(|i| format!("`{i}`"))
    .collect::<Vec<_>>()
    .join(", ");

  let validator_macro_name =
    InternMacro::InputValidation(struct_name, struct_name.span()).to_ident();

  let error_message = format!(
    "invalid field provided. Valid fields for `{struct_name}` are: \
     {valid_fields_list_str}."
  );
  quote! {
      macro_rules! #validator_macro_name {

          #( #valid_field_arms )*

          //  catch-all reached. Input key is not defined on this struct, thus we cry
          ($other:ident) => { compile_error!(concat!(#error_message)) };
      }
  }
}

fn validate_and_get_usr_input(
  user_args: &AutoAssignMacro
) -> crate::Result<HashMap<&Ident, &Expr>> {
  Ok(
    user_args
      .iter()
      .try_fold(HashMap::new(), |mut map, kv| {
        // if key is unique, insert returns `None`.
        if let Some(old_entry) = map.insert(kv.key.to_string(), kv) {
          Err(crate::Errors::new_at(
            old_entry.key.span(),
            "Repeated Assignment not allowed",
          ))
        } else {
          Ok(map)
        }
      })?
      .iter()
      .fold(HashMap::new(), |mut map, entry| {
        map.insert(&entry.1.key, &entry.1.value);
        map
      }),
  )
}

fn wrap_option_type(
  id: &Ident,
  ty: &syn::Type,
) -> TokenStream2 {
  match ty {
    syn::Type::Path(type_path)
      if type_path.qself.is_none()
        && type_path
          .path
          .segments
          .last()
          .is_some_and(|seg| seg.ident == "Option") =>
    {
      quote! { // Use the trait from `get_type_handler_trait_ts()` to handle option type
        ($self:expr, #id, [#id = $val:expr $(, $($rest:tt)*)?]) => {OptionWrapper::wrap($val)};
      }
    },
    _ => {
      // This gives us some flexibility in the builder struct. Sometimes
      //  we want to autofill a field with a default value if it's supported by
      //  our domain logic -> Option type on struct field becomes optional
      quote! {($self:expr, #id, [#id = $val:expr $(, $($rest:tt)*)?]) => { $val };}
    },
  }
}
