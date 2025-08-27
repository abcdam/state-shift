use core::ops::Deref;

use quote::ToTokens;
use syn::{
  parse::{Parse, ParseStream},
  punctuated::Punctuated,
  Attribute,
  Expr,
  Ident,
};

use crate::prelude::{
  external::{String, ToString, Vec},
  extra_macros as m,
};

// === Generalized Parser types ===
/// just an alias to reduce verbosity
type InnerCsvList<T> = Punctuated<T, m::Token![,]>;

/// newtype that represents a comma separated list of (generic) items
pub struct CsvList<T>(pub InnerCsvList<T>);

/// type representing '(entry, entry,..)' tokens
pub struct ParenCsvList<T>(pub CsvList<T>);

/// type representing 'KEY = VALUE' tokens
pub struct KeyValueAssignment<T = Expr> {
  pub key:   Ident,
  eq_token:  m::Token![=],
  pub value: T,
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

/// expose all functions of our inner newtype value
impl<T> Deref for CsvList<T> {
  type Target = InnerCsvList<T>;

  fn deref(&self) -> &Self::Target { &self.0 }
}
impl<T> Deref for ParenCsvList<T> {
  type Target = InnerCsvList<T>;

  fn deref(&self) -> &Self::Target { &self.0 }
}
impl Deref for AutoAssignArgs {
  type Target = CsvList<KeyValueAssignment>;

  fn deref(&self) -> &Self::Target { &self.0 }
}

/// Implement Debug ergonomics for all ToToken trait implementers inside our newtype
impl<T: ToTokens> core::fmt::Debug for CsvList<T> {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    let items: Vec<String> = self
      .0
      .iter()
      .map(|item| item.to_token_stream().to_string())
      .collect();
    f.debug_tuple("CsvList").field(&items).finish()
  }
}

/// allows us to turn our KV struct into token streams
impl<T: ToTokens> ToTokens for KeyValueAssignment<T> {
  fn to_tokens(
    &self,
    tokens: &mut proc_macro2::TokenStream,
  ) {
    let key = &self.key;
    let eq = &self.eq_token;
    let value = &self.value;
    m::quote! { #key #eq #value }.to_tokens(tokens);
  }
}

impl<T: ToTokens> ToTokens for ParenCsvList<T> {
  fn to_tokens(
    &self,
    tokens: &mut proc_macro2::TokenStream,
  ) {
    let items = &self.0.to_token_stream();
    m::quote! { (#items) }.to_tokens(tokens);
  }
}
impl ToTokens for AutoAssignArgs {
  fn to_tokens(
    &self,
    tokens: &mut proc_macro2::TokenStream,
  ) {
    self.0.to_tokens(tokens);
  }
}
/// add Debug ergonomics for KV struct (that's why we require to_token_stream() on the syn-type fields)
impl core::fmt::Debug for KeyValueAssignment {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.debug_struct("KeyValueAssignment")
      .field("key", &self.key)
      .field("value", &self.value.to_token_stream().to_string())
      .finish()
  }
}

impl core::fmt::Debug for TypeStateArgs {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.debug_struct("TypeStateArgs")
      .field("states", &self.states.to_token_stream().to_string())
      .field("slots", &self.slots.to_token_stream().to_string())
      .finish()
  }
}

impl<T: Parse> Parse for ParenCsvList<T> {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    let content;
    m::parenthesized!(content in input);
    Ok(ParenCsvList(content.parse()?))
  }
}

/// if T can be parsed, so can CsvList<T>
impl<T: Parse> Parse for CsvList<T> {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    // parse_separated_nonempty requires at least one element
    Ok(CsvList(Punctuated::parse_terminated(input)?))
  }
}

impl<T: Parse> Parse for KeyValueAssignment<T> {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    Ok(KeyValueAssignment {
      key:      input.parse()?,
      eq_token: input.parse()?,
      value:    input.parse()?,
    })
  }
}

impl Parse for TypeStateArgs {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    let args: CsvList<KeyValueAssignment<ParenCsvList<Ident>>> =
      input.parse()?;

    let (states, slots, mut err_acc) = args.0.into_iter().fold(
      (None, None, crate::Errors::new()),
      |(mut states, mut slots, mut errors), kv| {
        match kv.key.to_string().as_str() {
          "states" if states.is_none() => states = Some(kv.value),

          "slots" if slots.is_none() => slots = Some(kv.value),
          "slots" | "states" => {
            errors.push(syn::Error::new(kv.key.span(), "duplicate keys"));
          },
          _ => {
            errors.push(syn::Error::new(kv.key.span(), "unsupported keys"));
          },
        }
        (states, slots, errors)
      },
    );
    if states.is_none() {
      err_acc.push(syn::Error::new(input.span(), "missing `states`"));
    }
    if slots.is_none() {
      err_acc.push(syn::Error::new(input.span(), "missing `slots`"));
    }
    if err_acc.is_some() {
      Err(combine_parse_errors(err_acc))
    } else {
      Ok(Self {
        states: states.unwrap(),
        slots:  slots.unwrap(),
      })
    }
  }
}

impl Parse for AutoAssignArgs {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    Ok(AutoAssignArgs(input.parse()?))
  }
}
fn combine_parse_errors(e: crate::Errors) -> syn::Error {
  // only call this when e is non-empty
  e.0
    .into_iter()
    .reduce(|mut a, b| {
      a.combine(b);
      a
    })
    .expect("combine_errors called with empty Errors")
}

pub struct TypeStateArgs {
  pub states: ParenCsvList<Ident>,
  pub slots:  ParenCsvList<Ident>,
}
#[derive(Debug)]
pub struct AutoAssignArgs(CsvList<KeyValueAssignment>);
// === Type definitions for attribute parsing consumed by #[impl_state] ===
/// Generalize over all optional macros that are consumed by #[impl_state]
/// - Central place enforcing required type operations and macro identifiers
pub trait IsSupportedMacro {
  type Args: Parse + Sized + core::fmt::Debug;
  const MACRO_NAME: &'static str;
}

/// #[type_state(...)]
pub struct TypeStateMacro;
impl IsSupportedMacro for TypeStateMacro {
  type Args = TypeStateArgs;

  const MACRO_NAME: &'static str = "type_state";
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
  type Args = AutoAssignArgs;

  const MACRO_NAME: &'static str = "auto_assign";
}

pub fn parse_macro_args<M: IsSupportedMacro>(
  tokens: proc_macro::TokenStream
) -> Result<M::Args, syn::Error> {
  syn::parse::<M::Args>(tokens)
}

/// Helper function to find and remove an attribute by name
fn find_and_remove_attr(
  attrs: &mut Vec<Attribute>,
  attr_name: &str,
) -> Option<Attribute> {
  attrs
    .iter()
    .position(|attr| attr.path().is_ident(attr_name))
    .map(|pos| attrs.remove(pos))
}

/// Extracts the arguments from a macro call
pub fn extract_macro_args<T>(
  attrs: &mut Vec<Attribute>
) -> crate::Result<Option<<T as IsSupportedMacro>::Args>>
where
  T: IsSupportedMacro,
{
  find_and_remove_attr(attrs, T::MACRO_NAME)
    .map(|attr| attr.parse_args().map_err(crate::Errors::from))
    .transpose()
}

pub fn is_single_letter(ident: &Ident) -> bool { ident.to_string().len() == 1 }
