pub mod extra_macros {
  pub use quote::{format_ident, quote};
  pub use syn::{parenthesized, parse_macro_input, Token};
}

pub mod external {
  extern crate alloc;
  pub use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
  };
}
