use proc_macro2::TokenStream;
use quote::format_ident;
use syn::{punctuated::Punctuated, token::Comma, Field, Ident, Type};

/// Unique per struct identifier for the internal macro entrypoint
pub fn get_return_struct_macro_id(struct_name: &Ident) -> Ident {
  Ident::new(
    &format!("__state_shift_return_for_{struct_name}"),
    struct_name.span(),
  )
}
/// internal macro generator to construct returned struct
pub fn setup_return_type_macro(
  struct_name: &Ident,
  struct_fields: &Punctuated<Field, Comma>,
) -> TokenStream {
  // return a single TokenStream `assign_macros_ts` that contains all assembled `macro_rules! __assign_*` defs
  let mut assign_macros: Vec<TokenStream> = Vec::new();
  let mut field_inits_assign: Vec<TokenStream> = Vec::new(); // for ($s:expr, $($pairs:tt)*) arm

  for field in struct_fields.iter() {
    let fname_ident =
      field.ident.as_ref().expect("unnamed fields not supported");

    let is_option_kind = matches!(&field.ty, Type::Path(tp)
      if tp.path.segments.last().map(|seg| seg.ident == "Option").unwrap_or(false)
    );

    // unique assign macro per struct field
    let assign_ident = format_ident!(
      "__assign_{}_{}",
      struct_name.to_string(),
      fname_ident.to_string()
    );
    let handled_tokens = if is_option_kind {
      quote::quote! { Some($val) }
    } else {
      quote::quote! { $val }
    };

    let assign_macro = quote::quote! {
        macro_rules! #assign_ident {
            // head matches `field = $val, ...`
            ($s:expr, #fname_ident = $val:expr, $($rest:tt),*) => { #handled_tokens };

            // last `field = $val`
            ($s:expr, #fname_ident = $val:expr) => { #handled_tokens };

            // other head: skip and recurse
            ($s:expr, $other:ident = $val:expr, $($rest:tt),*) => { #assign_ident!($s, $($rest)*) };

            // skip last non-matching item
            ($s: expr, $other: ident = $val: expr) => { #assign_ident!($s) };

            // nothing matched: fall back to original $s.field
            ($s:expr) => { $s.#fname_ident };
        }
    };
    assign_macros.push(assign_macro);
    let init_assign = quote::quote! {
        #fname_ident: #assign_ident!($s, $($pairs)*)
    };
    field_inits_assign.push(init_assign);
  }
  let mut assign_macros_ts = TokenStream::new();
  for part in assign_macros {
    assign_macros_ts.extend(part);
  }
  let helper_ident = get_return_struct_macro_id(struct_name);

  let macro_ts = quote::quote! {
  #assign_macros_ts
      macro_rules! #helper_ident {

          // updates: arbitrary list of `struct.field = expr` pairs
          ($s:expr, $state_expr: expr, $($pairs:tt)*) => {

              // The struct constructor
              #struct_name {
                  #(#field_inits_assign, )*
                  _state: $state_expr
              }
          };
      }
  };

  macro_ts
}
