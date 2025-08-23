use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use stringcase::snake_case;
use syn::{
  Ident,
  ItemStruct,
  WherePredicate,
  parse_macro_input,
  punctuated::Punctuated,
  spanned::Spanned,
  token::Comma,
};

use crate::{
  auto_assign::auto_assign_macro_factory,
  helper::{TypeStateMacro, parse_macro_args},
};

pub fn type_state_inner(
  args: TokenStream1,
  input: TokenStream1,
) -> TokenStream1 {
  let mut errors = crate::Errors::new();

  let input_struct = parse_macro_input!(input as ItemStruct);
  let struct_name = &input_struct.ident;
  let generics = &input_struct.generics;
  let visibility = &input_struct.vis;
  let attrs = input_struct.attrs;

  let type_state_args = match parse_macro_args::<TypeStateMacro>(args) {
    Ok(type_state_args) => type_state_args,
    Err(failure) => return errors.extend(failure.into()).to_compile_error(),
  };
  // Generate the marker structs and sealing traits
  let sealer_trait_name =
    Ident::new(&format!("Sealer{}", struct_name), struct_name.span());
  let sealed_mod_name = Ident::new(
    &format!("sealed_{}", snake_case(&struct_name.to_string())),
    struct_name.span(),
  );

  let (final_markers_ts, final_sealed_impls_ts, final_trait_impls_ts) =
    type_state_args.states.iter().fold(
      (
        TokenStream2::new(),
        TokenStream2::new(),
        TokenStream2::new(),
      ),
      |(markers_ts, sealed_impl, trait_impls), state_ident| {
        let marker_name =
          Ident::new(state_ident.to_string().as_str(), state_ident.span());
        (
          quote! {  #markers_ts pub struct #marker_name;  },
          quote! {  #sealed_impl impl #sealed_mod_name::Sealed for #marker_name {} },
          quote! {  #trait_impls impl #sealer_trait_name for #marker_name {} },
        )
      },
    );

  // Extract fields from the struct
  // we cannot use `input_struct.fields` directly because
  // quote! treats the Fields reference as a block expression,
  // leading to the generated fields being wrapped inside
  // an extra set of braces ({ ... }).
  let struct_fields = match retrieve_struct_fields(&input_struct.fields) {
    Ok(fields) => fields,
    Err(err) => return errors.extend(err).into(),
  };

  let (q_generics_assign_pairs, q_new_where_clause, q_phantom_fields): (
    Vec<_>,
    Vec<_>,
    Vec<_>,
  ) = type_state_args
    .slots
    .iter()
    .enumerate()
    .map(|(idx, slot_id)| {
      // Generate state generics: `struct StructName<PlayerState1, PlayerState2, ...>`
      let state_ident = Ident::new(
        &format!("{}State{}", struct_name, idx + 1),
        struct_name.span(),
      );
      (
        // default generic states
        quote! {#state_ident = #slot_id},
        // new where clause
        quote! {#state_ident: #sealer_trait_name},
        // Construct the `_state` field with PhantomData
        // `_state: PhantomData<fn() -> T>`
        // the reason for using `fn() -> T` is to: https://github.com/ozgunozerk/state-shift/issues/1
        quote!(::core::marker::PhantomData<fn() -> #state_ident>),
      )
    })
    .collect();

  // Construct the new generics by merging original generics with default states
  // let default_generics = type_state_args.slots.iter().collect::<Vec<_>>();
  let combined_generics = {
    let new_generics_iter =
      q_generics_assign_pairs.iter().filter_map(|gp_pair| {
        // You don't need to dereference `gp_pair` if it's a TokenStream
        syn::parse2::<syn::GenericParam>(gp_pair.clone())
          .map_err(|e| {
            errors.extend(
              (gp_pair.span(), format!("malformed Generic: {e}")).into(),
            )
          })
          .ok()
      });

    let all_generics: Vec<_> = generics
      .params
      .iter()
      .cloned()
      .chain(new_generics_iter)
      .collect();

    quote! { #(#all_generics),* }
  };
  if errors.is_some() {
    return errors.into();
  }

  let all_where_predicates: Vec<_> = generics
    .where_clause
    .iter()
    .flat_map(|wc| wc.predicates.iter())
    .cloned()
    .chain(
      q_new_where_clause
        .iter()
        .filter_map(|ts| {
          syn::parse2::<WherePredicate>(ts.clone())
            .map_err(|e| {
              errors.extend(
                (ts.span(), format!("malformed WhereClause: {e}")).into(),
              )
            })
            .ok() // we collect Err results and yield later
        })
        .collect::<Vec<_>>(), // evaluate new where predicates for errors
    )
    .collect();

  let merged_where_clause_ts = if !all_where_predicates.is_empty() {
    quote! { where #(#all_where_predicates,)* }
  } else {
    quote! {}
  };

  // generate internal macro that is invoked on associated functions
  //    declarting `#[auto_assign(...)]` -> maybe introduce a flag to toggle this feature
  let auto_assign_ts =
    match auto_assign_macro_factory(struct_name, struct_fields) {
      Ok(ts) => ts,
      Err(e) => return errors.extend(e.into()).into(),
    };

  // Generate the final output
  let output = quote! {
      mod #sealed_mod_name {
          pub trait Sealed {}
      }

      pub trait #sealer_trait_name: #sealed_mod_name::Sealed {}

      #final_markers_ts
      #final_sealed_impls_ts
      #final_trait_impls_ts

      #(#attrs)*
      #[allow(clippy::type_complexity)]
      #visibility struct #struct_name<#combined_generics>
      #merged_where_clause_ts
      {
          #struct_fields
          _state: (#(#q_phantom_fields),*),
      }
      #auto_assign_ts
  };

  output.into()
}

fn retrieve_struct_fields(
  fields: &syn::Fields
) -> crate::Result<&Punctuated<syn::Field, Comma>> {
  match fields {
    syn::Fields::Named(named) => Ok(&named.named),
    other => Err((other.span(), "Struct must have named fields").into()),
  }
}
