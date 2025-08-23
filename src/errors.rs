use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::Error as SynError;

/// Aggregator for syn::Error we can push to
/// emit them as compile errors at the end.
#[derive(Default, Debug)]
pub(crate) struct Errors(pub Vec<SynError>);

impl Errors {
  pub(crate) fn new() -> Self { Self(Vec::new()) }

  /// single stateless error
  pub(crate) fn new_at<S: core::convert::Into<Span>, T: core::fmt::Display>(
    span: S,
    msg: T,
  ) -> Self {
    syn::Error::new(span.into(), msg).into()
  }

  pub(crate) fn absorb<T: core::default::Default>(
    &mut self,
    result: Result<T>,
  ) -> T {
    match result {
      Ok(val) => val,
      Err(errs) => {
        self.extend(errs);
        T::default()
      },
    }
  }

  /// push a `syn::Error`.
  pub(crate) fn push(
    &mut self,
    e: SynError,
  ) -> &mut Self {
    self.0.push(e);
    self
  }

  pub(crate) fn is_some(&self) -> bool { !self.0.is_empty() }

  /// self-extend with other of same kind errors.
  pub(crate) fn extend(
    &mut self,
    mut other: Errors,
  ) -> &mut Self {
    self.0.append(&mut other.0);
    self
  }

  /// `proc_macro::TokenStream` containing all collected compile errors
  pub(crate) fn to_compile_error(&self) -> TokenStream1 {
    self
      .0
      .iter()
      .fold(TokenStream2::new(), |mut acc, e| {
        acc.extend(e.to_compile_error());
        acc
      })
      .into()
  }
}

impl<T> From<(Span, T)> for Errors
where
  T: core::fmt::Display,
{
  fn from(value: (Span, T)) -> Self { syn::Error::new(value.0, value.1).into() }
}
/// Convenient conversions
impl From<SynError> for Errors {
  fn from(e: SynError) -> Self { Errors(vec![e]) }
}

impl From<Errors> for TokenStream1 {
  fn from(e: Errors) -> TokenStream1 { e.to_compile_error() }
}
impl From<&mut Errors> for TokenStream1 {
  fn from(e: &mut Errors) -> TokenStream1 { e.to_compile_error() }
}

pub(crate) type Result<T> = std::result::Result<T, Errors>;
