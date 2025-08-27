use proc_macro::TokenStream;
use syn::{ImplItem, ItemImpl, Type};

use crate::{
  extract_macro_args,
  generate_impl_block_for_method_based_on_require_args,
  helper,
  prelude::{external::Vec, extra_macros as m},
};

pub fn impl_state_inner(item: TokenStream) -> TokenStream {
  let mut errors = crate::Errors::new();
  // Parse the impl block
  let mut input = m::parse_macro_input!(item as ItemImpl);

  // Extract the type name and generics of the struct being implemented
  let (struct_name, struct_generics) = match *input.self_ty {
    Type::Path(ref type_path) => {
      let last_segment = type_path.path.segments.last().unwrap();
      let struct_name = last_segment.ident.clone();
      let struct_generics = &last_segment.arguments;
      (struct_name, struct_generics)
    },
    _ => panic!("Unsupported type for impl block"),
  };

  // Extract the methods from the impl block
  let mut methods = Vec::new();

  for item in input.items.iter_mut() {
    if let ImplItem::Fn(method) = item {
      // Extract `#[require]` arguments if they exist
      // let require_args = extract_macro_args(&mut method.attrs, "require");
      // Generate the impl block for the method based on the extracted #[require] arguments
      let modified_method =
        match extract_macro_args::<helper::RequiredMacro>(&mut method.attrs) {
          Ok(Some(detected_args)) => {
            errors.absorb(generate_impl_block_for_method_based_on_require_args(
              method,
              &struct_name,
              &detected_args,
              &input.generics,
              struct_generics,
            ))
          },
          Ok(None) => m::quote! { #method },
          Err(err) => return errors.extend(err).to_compile_error(),
        };

      // Push the modified method to the list of methods
      methods.push(modified_method);
    }
  }
  if errors.is_some() {
    errors.to_compile_error()
  } else {
    // Generate the expanded code with unique modules and traits
    let expanded = m::quote! {
        #(#methods)*
    };

    expanded.into()
  }
}
