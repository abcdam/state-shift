/// this file contains the logic that modifies the methods that are annotated with `#[require]` macro,
/// however, all the functions inside this file will be used by `#[impl_state]` macro due to delegation needs
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
  punctuated::Punctuated,
  spanned::Spanned,
  token::Comma,
  Expr,
  ExprStruct,
  GenericArgument,
  GenericParam,
  Ident,
  Member,
  Token,
  TypeParam,
  WhereClause,
  WherePredicate,
};

use crate::{helper::RequiredMacro, is_single_letter};

pub fn modify_struct_in_expr(
  expr: &Expr,
  struct_name: &syn::Ident,
  phantom_expr: TokenStream,
) -> Option<Expr> {
  match expr {
    Expr::Struct(expr_struct) if expr_struct.path.is_ident(struct_name) => {
      // Clone the struct fields and add the `_state` field
      let mut new_fields = expr_struct.fields.clone();
      new_fields.push(syn::FieldValue {
        attrs:       Vec::new(),
        member:      Member::Named(syn::Ident::new(
          "_state",
          struct_name.span(),
        )),
        colon_token: Some(<Token![:]>::default()),
        expr:        Expr::Verbatim(phantom_expr.clone()),
      });

      // Return a modified struct expression with the new fields
      Some(Expr::Struct(ExprStruct {
        fields: new_fields,
        ..expr_struct.clone()
      }))
    },
    // If it's an expression like `Some(Player { ... })` or `Ok(Player { ... })`
    Expr::Call(call_expr) => {
      let mut new_args = vec![];
      let mut modified = false;

      for arg in &call_expr.args {
        let phantom = phantom_expr.clone();
        if let Some(modified_arg) =
          modify_struct_in_expr(arg, struct_name, phantom)
        {
          new_args.push(modified_arg);
          modified = true;
        } else {
          new_args.push(arg.clone());
        }
      }

      if modified {
        Some(Expr::Call(syn::ExprCall {
          args: new_args.into_iter().collect(),
          ..call_expr.clone()
        }))
      } else {
        None
      }
    },
    _ => None,
  }
}

pub struct ImplFromRequired<'a> {
  pub src_impl_generics: &'a syn::Generics,
  pub _input_fn_ident:   &'a Ident,
  pub struct_name:       &'a Ident,
  pub struct_generics:   &'a Punctuated<GenericArgument, Comma>,
  pub required_state:    &'a RequiredMacro,
}
pub struct ImplFromRequiredResult {
  pub combined_struct_generics: Punctuated<GenericArgument, Comma>,
  pub all_impl_block_generics:  Punctuated<GenericParam, Comma>,
  pub impl_block_where_clause:  Option<WhereClause>,
  pub phantom_state_expr:       TokenStream,
}
pub fn build_impl_block_signature(
  args: &ImplFromRequired
) -> crate::Result<ImplFromRequiredResult> {
  // Convert the struct's generics into a Punctuated collection
  let combined_struct_generics: Punctuated<GenericArgument, Comma> = args
    .struct_generics
    .clone()
    .into_iter()
    // Append the full list of arguments from `#[require]` macro: (A, B, State1, ...)
    .chain(args.required_state.iter().map(|ident| {
    // Convert each parsed argument into a GenericArgument (which is a TypeParam)
    syn::GenericArgument::Type(syn::Type::Path(syn::TypePath {
      qself: None,
      path:  syn::Path::from(ident.clone()), // Use the ident for the type path
    }))
  })).collect();

  // put the sealed trait boundary for the generics:
  // ``` where
  // A: Sealer,
  // B: Sealer,
  let sealer_trait_name = Ident::new(
    &format!("Sealer{}", args.struct_name),
    args.struct_name.span(),
  );
  let new_where_clause_predicates: Vec<WherePredicate> = args
    .src_impl_generics
    .where_clause
    .iter()
    .flat_map(|wc| wc.predicates.iter())
    .cloned()
    .chain(
      args
        .required_state
        .iter()
        .filter(|ident| is_single_letter(ident))
        .filter_map(|ident| {
          syn::parse2::<syn::WherePredicate>(
            quote! {#ident: #sealer_trait_name},
          )
          .ok()
        })
        .collect::<Vec<_>>(),
    )
    .collect();
  // Merge with the existing where clause, if any.
  let impl_block_where_clause = if !new_where_clause_predicates.is_empty() {
    Some(syn::parse2::<syn::WhereClause>(
      quote! { where #(#new_where_clause_predicates,)* },
    )?)
  } else {
    None
  };
  // Generate PhantomData for the required number of states
  let phantom_data: Vec<_> = (0..args.required_state.len())
    .map(|_| quote!(::core::marker::PhantomData))
    .collect();

  let all_impl_block_generics: Punctuated<GenericParam, Comma> = args
    .src_impl_generics
    .params
    .iter()
    .cloned()
    .chain(
      args
        .required_state
        .iter()
        .filter(|ident| is_single_letter(ident))
        .map(|g_ident| GenericParam::Type(TypeParam::from(g_ident.clone())))
        .collect::<Punctuated<GenericParam, Comma>>(),
    )
    .collect();
  let phantom_state_expr = if phantom_data.len() == 1 {
    quote! { ::core::marker::PhantomData }
  } else {
    quote! { ( #(#phantom_data),* ) }
  };

  Ok(ImplFromRequiredResult {
    combined_struct_generics,
    all_impl_block_generics,
    impl_block_where_clause,
    phantom_state_expr,
  })
}

pub fn extract_struct_ident_and_generics(
  input: syn::ItemImpl
) -> crate::Result<(syn::Ident, Punctuated<GenericArgument, syn::token::Comma>)>
{
  match input.self_ty.as_ref() {
    syn::Type::Path(type_path) => {
      let last_segment = match type_path.path.segments.last() {
        Some(seg) => seg,
        None => {
          return Err(crate::Errors::new_at(
            input.self_ty.span(),
            "Unsupported type for impl block",
          ))?
        },
      };

      let struct_name = &last_segment.ident;

      let struct_generics = match &last_segment.arguments {
        syn::PathArguments::None => Punctuated::new(),
        syn::PathArguments::AngleBracketed(angle) => angle.args.clone(),
        syn::PathArguments::Parenthesized(p) => {
          return Err(crate::Errors::new_at(
            p.span(),
            "Unsupported generics format for struct",
          ))?
        },
      };

      Ok((struct_name.clone(), struct_generics))
    },
    _ => Err(crate::Errors::new_at(
      input.self_ty.span(),
      "Unsupported type for impl block",
    ))?,
  }
}
