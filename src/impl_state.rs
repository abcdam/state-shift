use proc_macro::TokenStream;
use quote::quote;
use syn::{
  punctuated::Punctuated,
  token::Comma,
  GenericArgument,
  Generics,
  ImplItem,
  ImplItemFn,
  ItemImpl,
  Stmt,
};

use crate::{
  auto_assign::process_auto_assign,
  helper::{parse_macros, MacroConfig},
  require::{
    build_impl_block_signature,
    extract_struct_ident_and_generics,
    ImplFromRequired,
    ImplFromRequiredResult,
  },
  switch_to_inner,
};
/// [#impl_state] anchor -> consume all function attributes
pub fn impl_state_inner(item: TokenStream) -> TokenStream {
  let mut input = syn::parse_macro_input!(item as ItemImpl);
  let (struct_name, struct_generics) =
    match extract_struct_ident_and_generics(input.clone()) {
      Ok(pair) => pair,
      Err(comp_err) => return comp_err.to_compile_error(),
    };
  let input_generics = input.generics;
  let rendered_methods = input
    .items
    .iter_mut()
    .map(|item| {
      if let ImplItem::Fn(method) = item {
        let parsed_macro_args = parse_macros(method)?;

        let outcome_opt = RenderOutcome::from_macros(
          &input_generics,
          &struct_name,
          &struct_generics,
          method,
          parsed_macro_args,
        )?;
        match outcome_opt {
          Some(success) => Ok(success.to_token_stream(&struct_name, method)),
          None => Ok(quote! {}),
        }
      } else {
        Ok(quote! {})
      }
    })
    .collect::<crate::Result<Vec<_>>>();
  match rendered_methods {
    Ok(rendered_ts) => quote! { #(#rendered_ts)* }.into(),
    Err(err) => err.to_compile_error(),
  }
}

struct RenderOutcome {
  /// #[require(...)] is the base case.
  base: ImplFromRequiredResult,

  /// #[switch_to(...)] and in case it's not set, it switches to the input of #[require(...)]
  switch_to_return: syn::ReturnType,

  /// If present, #[auto_assign(...)] is injected as last expression of function body.
  ///   #require and #switch_to must be set
  auto_assign_tail: Option<proc_macro2::TokenStream>,
}

impl RenderOutcome {
  /// Determine the outcome from the parsed macros and method context.
  fn from_macros(
    src_impl_generics: &Generics,
    struct_name: &proc_macro2::Ident,
    struct_generics: &Punctuated<GenericArgument, Comma>,
    method: &ImplItemFn,
    macro_opt: MacroConfig,
  ) -> Result<Option<Self>, crate::Errors> {
    // nothing to do if require is not present. ignore other macros
    let required_state = match macro_opt.require {
      None => return Ok(None),
      Some(r) => r,
    };

    // Build the required impl signature first
    let required_args = ImplFromRequired {
      src_impl_generics,
      _input_fn_ident: &method.sig.ident,
      struct_name,
      struct_generics,
      required_state: &required_state,
    };

    let base = build_impl_block_signature(&required_args)?;

    // Generate the impl block for the method based on the extracted #[switch_to] arguments
    let switch_to_return = match &macro_opt.switch_to {
      Some(s) => {
        // switch_to_inner expects (method.sig.output, &switch_to_state.0, ...)
        switch_to_inner(
          &method.sig.output,
          &s.0,
          required_args.struct_name,
          &method.sig.ident,
        )
      },
      // there is no `#[switch_to]` macro, so we use the `#[require]` macro's arguments instead
      // to keep the type same for the input and the output
      None => switch_to_inner(
        &method.sig.output,
        &required_args.required_state.0,
        required_args.struct_name,
        &method.sig.ident,
      ),
    };

    // If auto_assign was supplied, we must have switch_to present — the
    // parse_macros function already enforces this, but double check here.
    let auto_assign_tail = if let Some(auto_assign_args) = macro_opt.auto_assign
    {
      if macro_opt.switch_to.is_none() {
        Err(crate::Errors::new_at(
          method.sig.ident.span(),
          "#[auto_assign] requires #[switch_to]",
        ))?
      }

      // derive/generate return instance based on [#auto_assign(..)]
      Some(process_auto_assign(
        &method.sig.ident,
        required_args.struct_name,
        base.phantom_state_expr.clone(),
        auto_assign_args,
      )?)
    } else {
      None
    };

    Ok(Some(Self {
      base,
      switch_to_return,
      auto_assign_tail,
    }))
  }

  // assemble the final render result from all collected tokenstream snippets
  fn to_token_stream(
    &self,
    struct_ident: &proc_macro2::Ident,
    method: &mut ImplItemFn,
  ) -> proc_macro2::TokenStream {
    let ImplFromRequiredResult {
      combined_struct_generics,
      all_impl_block_generics,
      impl_block_where_clause,
      phantom_state_expr,
    } = &self.base;

    let fn_body = if let Some(auto_assign_factory_invocation) =
      &self.auto_assign_tail
    {
      let stmts = method.block.stmts.clone();
      quote! {
          #(#stmts)*
          #auto_assign_factory_invocation
      }
    } else {
      let new_fn_body: Vec<Stmt> = method
        .block
        .stmts
        .iter()
        .map(|stmt| {
          if let Stmt::Expr(expr, maybe_semi) = stmt {
            if let Some(modified_expr) = crate::require::modify_struct_in_expr(
              expr,
              struct_ident,
              phantom_state_expr.clone(),
            ) {
              return Stmt::Expr(modified_expr, *maybe_semi);
            }
          }
          stmt.clone()
        })
        .collect();

      quote! { #(#new_fn_body)* }
    };

    // Possibly override the function signature output to make the type state magic do its work
    let fn_sig = &mut method.sig;
    fn_sig.output = self.switch_to_return.clone();

    let fn_vis = &method.vis;
    let ignored_attribs = &method.attrs;

    let base_snippet = quote! {
        impl<#all_impl_block_generics> #struct_ident<#combined_struct_generics>
            #impl_block_where_clause
    };

    quote! {
        #base_snippet
        {
            #(#ignored_attribs)*
            #fn_vis #fn_sig {
                #fn_body
            }
        }
    }
  }
}
