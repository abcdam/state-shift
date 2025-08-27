use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;
use syn::{Attribute, Expr, Ident};

use crate::{
  extract_macro_args,
  helper::{AutoAssignArgs, AutoAssignMacro},
  prelude::extra_macros as m,
};
const MACRO_PREFIX: &str = "__state_shift_auto_assign";

/// Unique per struct identifier for the internal factory entrypoint
pub fn get_struct_factory_ident(struct_name: &Ident) -> Ident {
  Ident::new(&format!("{MACRO_PREFIX}_{struct_name}"), struct_name.span())
}
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
  let field_setter_macro_ts =
    generate_fields_overrider_macro_ts(&field_idents, &field_types);
  let input_validator_macro_ts =
    generate_input_validation_macro_ts(struct_name, &field_idents);
  let helper_trait_ts = get_type_handler_trait_ts();
  let builder_macro_name = get_struct_factory_ident(struct_name);
  let factory = m::quote! {
    #[allow(unused_macros)]
    #input_validator_macro_ts

    #[allow(unused_macros)]
    #field_setter_macro_ts

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
  all_attributes: &mut Vec<Attribute>,
  phantom_state_field: TokenStream2,
) -> crate::Result<TokenStream2> {
  let assign_attr = match extract_macro_args::<AutoAssignMacro>(all_attributes)?
  {
    Some(args) => args,
    None => return Ok(m::quote! {}),
  };
  let validator_macro_name =
    m::format_ident!("__validate_fields_of_{}", struct_name);
  // validator_macro_name.set_span(func_ident.span());
  let usr_assignments = validate_and_get_usr_input(&assign_attr)?;
  let validation_calls: Vec<_> = usr_assignments
    .keys()
    .map(|&k| m::quote! {#validator_macro_name!(#k);})
    .collect();
  let generated_code = m::quote! {
      const _: () = {#( #validation_calls )*};
  };

  let kv_arms: Vec<_> = usr_assignments
    .into_iter()
    .map(|(field_to_update, expression_to_assign)| {
      m::quote! {#field_to_update = #expression_to_assign
      }
    })
    .collect();

  let mut builder_macro_name = get_struct_factory_ident(struct_name);
  builder_macro_name.set_span(func_ident.span());
  Ok(m::quote! {
    #generated_code
    #builder_macro_name!(self, #phantom_state_field, #(#kv_arms),* )
  })
}

fn get_type_handler_trait_ts() -> TokenStream2 {
  m::quote! {
    trait OptionWrapper<T> {
      fn wrap(self) -> Option<T>;
    }
    impl<T> OptionWrapper<T> for T {
      fn wrap(self) -> Option<T> {
          Some(self)
      }
    }
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
  let override_arms = f_idents
    .iter()
    .zip(f_types.iter())
    .map(|(id, ty)| wrap_option_type(id, ty));

  m::quote! {
    macro_rules! default_or_override {
      #(#override_arms)*
      ($self:expr, $field:ident, [$other:ident = $val:expr, $($rest:tt)*]) => {
        default_or_override!($self, $field, [$($rest)*])
      };
      ($self:expr, $field:ident, [$other:ident = $val:expr]) => {default_or_override!($self, $field,[])};
      ($self:expr, $field:ident,[]) => {$self.$field};

     }
  }
}

fn generate_input_validation_macro_ts(
  struct_name: &Ident,
  f_idents: &[Ident],
) -> TokenStream2 {
  let valid_field_arms = f_idents.iter().map(|ident| {
    m::quote! { (#ident) => {}; }
  });
  let valid_fields_list_str = f_idents
    .iter()
    .map(|i| format!("`{i}`"))
    .collect::<Vec<_>>()
    .join(", ");
  let validator_macro_name =
    m::format_ident!("__validate_fields_of_{}", struct_name);
  let error_message = format!(
    "invalid field provided. Valid fields for `{struct_name}` are: \
     {valid_fields_list_str}."
  );
  m::quote! {
      macro_rules! #validator_macro_name {
          #( #valid_field_arms )*
          ($other:ident) => { compile_error!(concat!(#error_message)) };
      }
  }
}

fn validate_and_get_usr_input(
  user_args: &AutoAssignArgs
) -> crate::Result<HashMap<&Ident, &Expr>> {
  Ok(
    user_args
      .iter()
      .try_fold(HashMap::new(), |mut map, kv| {
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
      m::quote! {
        ($self:expr, #id, [#id = $val:expr $(, $($rest:tt)*)?]) => {OptionWrapper::wrap($val)};
      }
    },
    _ => {
      m::quote! {($self:expr, #id, [#id = $val:expr $(, $($rest:tt)*)?]) => { $val };}
    },
  }
}
