use proc_macro2::TokenStream;
use quote::format_ident;
use syn::{Field, Ident, Type, punctuated::Punctuated, token::Comma};

/// Unique per struct identifier for the internal factory entrypoint
pub fn get_struct_factory_ident(struct_name: &Ident) -> Ident {
  Ident::new(
    &format!("__state_shift_auto_assign_{struct_name}"),
    struct_name.span(),
  )
}

/// macro assembler for #[auto_assign(...)]
pub fn auto_assign_macro_factory(
  struct_id: &Ident,
  struct_fields: &Punctuated<Field, Comma>,
) -> syn::Result<TokenStream> {
  let entrypoint_ident = get_struct_factory_ident(struct_id);
  let (field_eval_callees, field_eval_callers) =
    struct_fields.iter().try_fold(
      (TokenStream::new(), TokenStream::new()),
      |(mut acc_l, mut acc_r),
       s_field|
       -> syn::Result<(TokenStream, TokenStream)> {
        let (callee, caller) = gen_eval_invoker_pair_for_field(
          &entrypoint_ident.to_string(),
          s_field,
        )?;
        acc_l.extend(callee);
        acc_r.extend(caller);
        Ok((acc_l, acc_r))
      },
    )?;

  Ok(quote::quote_spanned! { entrypoint_ident.span() =>
    #field_eval_callees
      macro_rules! #entrypoint_ident {
          ($s:expr, $state_expr: expr, $($pairs:tt)*) => {

              #struct_id {
                  #field_eval_callers
                  _state: $state_expr
              }
          };
      }
  })
}

// Private

fn is_option_type(ty: &Type) -> bool {
  if let Type::Path(type_path) = ty {
    type_path
      .path
      .segments
      .last()
      .map(|seg| seg.ident == "Option")
      .unwrap_or(false)
  } else {
    false
  }
}

fn create_list_expression_evaluator_ts(
  field_id: &Ident,
  field_assigner_id: &Ident,
  is_option_type: bool,
) -> TokenStream {
  let handled_token = if is_option_type {
    quote::quote! { Some($val) }
  } else {
    quote::quote! { $val }
  };
  // unique assign macro per struct field
  quote::quote! {
      macro_rules! #field_assigner_id {
          // head matches `field = $val, ...`
          ($s:expr, #field_id = $val:expr, $($rest:tt)*) => { #handled_token };

          // last `field = $val`
          ($s:expr, #field_id = $val:expr) => { #handled_token };

          // other head: skip and recurse
          ($s:expr, $other:ident = $val:expr, $($rest:tt)*) => { #field_assigner_id!($s, $($rest)*) };

          // skip last non-matching item
          ($s:expr, $other:ident = $val:expr) => { #field_assigner_id!($s) };

          // nothing matched: fall back to original $s.field
          ($s:expr) => { $s.#field_id };
      }
  }
}

fn invoke_struct_field_evaluator_ts(
  field_id: &Ident,
  field_assigner_id: &Ident,
) -> TokenStream {
  quote::quote! {
      #field_id: #field_assigner_id!($s, $($pairs)*),
  }
}

fn gen_eval_invoker_pair_for_field(
  assign_field_macro_ident_prefix: &str,
  struct_field: &Field,
) -> syn::Result<(TokenStream, TokenStream)> {
  let field_id = struct_field.ident.as_ref().ok_or_else(|| {
    syn::Error::new_spanned(
      struct_field,
      "tuple/unnamed fields are not supported by auto_assign",
    )
  })?;

  let assign_struct_field_id = format_ident!(
    "{}_{}",
    assign_field_macro_ident_prefix,
    field_id.to_string()
  );

  let struct_field_eval_invoker_ts =
    invoke_struct_field_evaluator_ts(field_id, &assign_struct_field_id);

  let args_evaluator_ts = create_list_expression_evaluator_ts(
    field_id,
    &assign_struct_field_id,
    is_option_type(&struct_field.ty),
  );
  Ok((args_evaluator_ts, struct_field_eval_invoker_ts))
}
