use std::ops::Deref;

use proc_macro::TokenTree;
use quote::ToTokens;
use syn::{
  parse::{Parse, ParseStream},
  punctuated::Punctuated,
  Attribute,
  Expr,
  Ident,
  Result,
  Token,
};

// === Generalized Parser types ===
/// just an alias to reduce verbosity
type InnerCsvList<T> = Punctuated<T, Token![,]>;

/// newtype that represents a comma separated list of (generic) items
pub struct CsvList<T>(pub InnerCsvList<T>);

/// type representing 'KEY = VALUE' tokens
pub struct KeyValueAssignment {
  pub key:   Ident,
  eq_token:  Token![=],
  pub value: Expr,
}
// === Various impl blocks for better DX ===
/// ergonomic creation of a K/V token
impl From<(Ident, Expr)> for KeyValueAssignment {
  fn from(value: (Ident, Expr)) -> Self {
    KeyValueAssignment {
      key:      value.0,
      value:    value.1,
      eq_token: syn::token::Eq::default(),
    }
  }
}

/// allows us to turn our KV struct into token streams
impl ToTokens for KeyValueAssignment {
  fn to_tokens(
    &self,
    tokens: &mut proc_macro2::TokenStream,
  ) {
    let key = &self.key;
    let eq = &self.eq_token;
    let value = &self.value;
    quote::quote! { #key #eq #value }.to_tokens(tokens);
  }
}
/// Implement Debug ergonomics for all ToToken trait implementers inside our newtype
impl<T: ToTokens> std::fmt::Debug for CsvList<T> {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> std::fmt::Result {
    let items: Vec<String> = self
      .0
      .iter()
      .map(|item| item.to_token_stream().to_string())
      .collect();
    f.debug_tuple("CsvList").field(&items).finish()
  }
}

/// expose all functions of our inner newtype value
impl<T> Deref for CsvList<T> {
  type Target = InnerCsvList<T>;

  fn deref(&self) -> &Self::Target { &self.0 }
}

/// add Debug ergonomics for KV struct (that's why we require to_token_stream() on the syn-type fields)
impl std::fmt::Debug for KeyValueAssignment {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> std::fmt::Result {
    f.debug_struct("KeyValueAssignment")
      .field("key", &self.key)
      .field("value", &self.value.to_token_stream().to_string())
      .finish()
  }
}

/// if T can be parsed, so can CsvList<T>
impl<T: Parse> Parse for CsvList<T> {
  fn parse(input: ParseStream) -> Result<Self> {
    // parse_separated_nonempty requires at least one element
    Ok(CsvList(Punctuated::parse_terminated(input)?))
  }
}

impl Parse for KeyValueAssignment {
  fn parse(input: ParseStream) -> Result<Self> {
    Ok(KeyValueAssignment {
      key:      input.parse()?,
      eq_token: input.parse()?,
      value:    input.parse()?,
    })
  }
}
// === Type definitions for attribute parsing consumed by #[impl_state] ===
/// Generalize over all optional macros that are consumed by #[impl_state]
/// - Central place enforcing required type operations and macro identifiers
pub trait IsSupportedMacro {
  type Args: Parse + Sized + std::fmt::Debug;
  const MACRO_NAME: &'static str;
}

/// #[require(...)]
pub struct RequiredMacro;
impl IsSupportedMacro for RequiredMacro {
  type Args = CsvList<Ident>;

  const MACRO_NAME: &'static str = "require";
}

/// #[switch_to(...)]
pub struct SwitchToMacro;
impl IsSupportedMacro for SwitchToMacro {
  type Args = CsvList<Ident>;

  const MACRO_NAME: &'static str = "switch_to";
}

/// #[auto_assign(...)]
pub struct AutoAssignMacro;
impl IsSupportedMacro for AutoAssignMacro {
  type Args = CsvList<KeyValueAssignment>;

  const MACRO_NAME: &'static str = "auto_assign";
}

/// Helper function to find and remove an attribute by name
fn find_and_remove_attr(
  attrs: &mut Vec<Attribute>,
  attr_name: &str,
) -> Option<Attribute> {
  let pos = attrs
    .iter()
    .position(|attr| attr.path().is_ident(attr_name))?;
  Some(attrs.remove(pos))
}

/// Extracts the arguments from a macro call
pub fn extract_macro_args<T>(
  attrs: &mut Vec<Attribute>
) -> Option<<T as IsSupportedMacro>::Args>
where
  T: IsSupportedMacro,
{
  let attr = find_and_remove_attr(attrs, T::MACRO_NAME)?;
  attr.parse_args().ok()
}

pub fn is_single_letter(ident: &Ident) -> bool { ident.to_string().len() == 1 }

pub fn extract_idents_from_group(
  token: &TokenTree,
  error_msg: &str,
) -> Vec<Ident> {
  match token {
    proc_macro::TokenTree::Group(group) => group
      .stream()
      .into_iter()
      .filter_map(|tt| {
        if let proc_macro::TokenTree::Ident(ident) = tt {
          Some(Ident::new(&format!("{}", ident), ident.span().into()))
        } else {
          None
        }
      })
      .collect(),
    _ => panic!("{}", error_msg),
  }
}
