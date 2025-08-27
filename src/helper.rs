use core::ops::Deref;

use quote::{quote, ToTokens};
use syn::{
  parenthesized,
  parse::{Parse, ParseStream},
  punctuated::Punctuated,
  spanned::Spanned,
  Expr,
  Ident,
  Token,
};

// === Generalized Parser types ===
/// just an alias to reduce verbosity
type InnerCsvList<T> = Punctuated<T, Token![,]>;

/// newtype that represents a comma separated list of (generic) items
pub struct CsvList<T>(pub InnerCsvList<T>);

/// type representing '(entry, entry,..)' tokens
pub struct ParenCsvList<T>(pub CsvList<T>);

/// type representing 'KEY = VALUE' tokens
pub struct KeyValueAssignment<T = Expr> {
  pub key:   Ident,
  eq_token:  Token![=],
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
macro_rules! deref_newtype {

      (<$($gen:tt),+> $self_ty:ty => $target_ty:ty) => {
        impl<$($gen),+> Deref for $self_ty {
            type Target = $target_ty;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
    ($self_ty:ty => $target_ty:ty) => {
        impl Deref for $self_ty {
            type Target = $target_ty;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}
deref_newtype!(<T> CsvList<T> => InnerCsvList<T>);
deref_newtype!(<T> ParenCsvList<T> => InnerCsvList<T>);
deref_newtype!(AutoAssignArgs => CsvList<KeyValueAssignment>);
deref_newtype!(RequiredMacro => CsvList<Ident>);
deref_newtype!(SwitchToMacro => CsvList<Ident>);
deref_newtype!(AutoAssignMacro => AutoAssignArgs);
// impl Deref for AutoAssignArgs {
//   type Target = CsvList<KeyValueAssignment>;

//   fn deref(&self) -> &Self::Target { &self.0 }
// }

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
    quote! { #key #eq #value }.to_tokens(tokens);
  }
}

impl<T: ToTokens> ToTokens for ParenCsvList<T> {
  fn to_tokens(
    &self,
    tokens: &mut proc_macro2::TokenStream,
  ) {
    let items = &self.0.to_token_stream();
    quote! { (#items) }.to_tokens(tokens);
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
    parenthesized!(content in input);
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

pub fn parse_macro_args<M: IsSupportedMacro>(
  tokens: proc_macro::TokenStream
) -> Result<<M as IsSupportedMacro>::Args, syn::Error> {
  syn::parse::<M::Args>(tokens)
}

#[derive(Debug)]
pub struct AutoAssignArgs(CsvList<KeyValueAssignment>);
// === Type definitions for attribute parsing consumed by #[impl_state] ===
/// Generalize over all optional macros that are consumed by #[impl_state]
/// - Central place enforcing required type operations and macro identifiers
pub trait IsSupportedMacro {
  type Args: Parse + Sized + core::fmt::Debug;
  const MACRO_NAME: &'static str;
  fn parse(attr: &syn::Attribute) -> syn::Result<Self>
  where
    Self: Sized;
}

macro_rules! supported_macro {
  ($name:ident, $args:ty, $lit:literal) => {
    #[derive(Debug)]
    pub struct $name(pub $args);

    impl IsSupportedMacro for $name {
      type Args = $args;

      const MACRO_NAME: &'static str = $lit;

      fn parse(attr: &syn::Attribute) -> syn::Result<Self> {
        Ok(Self(attr.parse_args::<$args>()?))
      }
    }

    impl $name {}
  };
}

supported_macro!(RequiredMacro, CsvList<Ident>, "require");
supported_macro!(SwitchToMacro, CsvList<Ident>, "switch_to");
supported_macro!(AutoAssignMacro, AutoAssignArgs, "auto_assign");
supported_macro!(TypeStateMacro, TypeStateArgs, "type_state");

pub fn is_single_letter(ident: &Ident) -> bool { ident.to_string().len() == 1 }

pub struct MacroConfig {
  pub require:     Option<RequiredMacro>,
  pub switch_to:   Option<SwitchToMacro>,
  pub auto_assign: Option<AutoAssignMacro>,
}

pub fn parse_macros(
  method: &mut syn::ImplItemFn
) -> crate::Result<MacroConfig> {
  let require = take_macro::<RequiredMacro>(&mut method.attrs)?;
  if require.is_none() {
    return Ok(MacroConfig {
      require:     None,
      switch_to:   None,
      auto_assign: None,
    });
  }

  let switch_to = take_macro::<SwitchToMacro>(&mut method.attrs)?;
  let auto_assign = take_macro::<AutoAssignMacro>(&mut method.attrs)?;

  if auto_assign.is_some() && switch_to.is_none() {
    Err(crate::Errors::new_at(
      method.span(),
      format!(
        "#[{}(...)] requires #[{}(...)]",
        AutoAssignMacro::MACRO_NAME,
        SwitchToMacro::MACRO_NAME
      ),
    ))
  } else {
    Ok(MacroConfig {
      require,
      switch_to,
      auto_assign,
    })
  }
}

// generic macro extractor. Slurps and removes from tokenstream
fn take_macro<M: IsSupportedMacro + Sized>(
  attrs: &mut Vec<syn::Attribute>
) -> syn::Result<Option<M>> {
  if let Some(idx) = attrs.iter().position(|a| a.path().is_ident(M::MACRO_NAME))
  {
    let attr = attrs.remove(idx);
    Ok(Some(M::parse(&attr)?))
  } else {
    Ok(None)
  }
}
