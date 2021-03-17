extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{FnArg, Pat, PatType};

#[proc_macro_attribute]
pub fn deno_op(_: TokenStream, item: TokenStream) -> TokenStream {
  let mut func = syn::parse_macro_input!(item as syn::ItemFn);
  assert!(func.sig.ident.to_string().starts_with("op_"));
  let is_async = func.sig.asyncness.is_some();
  let struct_name = {
    let ident = &func.sig.ident;
    let name = format!("{}_args", ident.to_string())
      .split('_')
      .map(|s| {
        if s.len() > 0 {
          let mut chars: Vec<char> = s.chars().collect();
          chars[0] = chars[0].to_uppercase().nth(0).unwrap();
          chars.into_iter().collect::<String>()
        } else {
          String::new()
        }
      })
      .collect::<Vec<_>>()
      .join("");
    Ident::new(&name, ident.span())
  };

  let mut op_state: Option<PatType> = None;
  let mut args: Vec<PatType> = vec![];
  let mut zero_copy: Option<PatType> = None;

  {
    let mut iter = func.sig.inputs.iter().peekable();
    let mut i = 0;
    while let Some(arg) = iter.next() {
      if let FnArg::Typed(arg) = arg {
        match &*arg.pat {
          Pat::Ident(ident) => {
            let name = ident.ident.to_string();
            if i == 0 && name.as_str() == "state" {
              op_state = Some(arg.clone());
            } else if iter.peek().is_none()
              && ((!is_async && name.as_str() == "zero_copy")
              || (is_async && name.as_str() == "bufs"))
            {
              zero_copy = Some(arg.clone());
            } else {
              args.push(arg.clone());
            }
          }
          _ => unreachable!(),
        }
        i += 1;
      } else {
        panic!()
      }
    }
  }

  func.sig.inputs.clear();
  if let Some(op_state) = op_state {
    func.sig.inputs.push_value(FnArg::Typed(op_state));
  } else {
    // TODO: maybe full path?
    let arg = if is_async {
      quote! { _: Rc<RefCell<OpState>> }
    } else {
      quote! { _: &mut OpState }
    };
    let arg = arg.into();
    let arg = syn::parse_macro_input!(arg as syn::FnArg);
    func.sig.inputs.push_value(arg);
  }
  func
    .sig
    .inputs
    .push_punct(syn::Token![,](Span::call_site())); // TODO: proper span?

  let mut arg_struct = None;

  if args.is_empty() {
    let arg = quote! { _: Value };
    let arg = arg.into();
    let arg = syn::parse_macro_input!(arg as syn::FnArg);
    func.sig.inputs.push_value(arg);
  } else if args.len() == 1 {
    func.sig.inputs.push_value(FnArg::Typed(args[0].clone()));
  } else {
    let arg = quote! { args: #struct_name };
    let arg = arg.into();
    let arg = syn::parse_macro_input!(arg as syn::FnArg);
    func.sig.inputs.push_value(arg);

    arg_struct = Some(quote! {
      pub struct #struct_name {
        #(#args),*
      }
    });
  }
  func
    .sig
    .inputs
    .push_punct(syn::Token![,](Span::call_site())); // TODO: proper span?

  if let Some(zero_copy) = zero_copy {
    func.sig.inputs.push_value(FnArg::Typed(zero_copy));
  } else {
    // TODO: maybe full path?
    let arg = if is_async {
      quote! { _: BufVec }
    } else {
      quote! { _: &mut [ZeroCopyBuf] }
    };

    let arg = arg.into();
    let arg = syn::parse_macro_input!(arg as syn::FnArg);
    func.sig.inputs.push_value(arg);
  }

  if arg_struct.is_some() {
    let struct_prop_names = args
      .iter()
      .map(|x| match &*x.pat {
        Pat::Ident(ident) => ident.ident.clone(),
        _ => unreachable!(),
      })
      .collect::<Vec<_>>();

    let destruct = quote! {
      let #struct_name { #(#struct_prop_names),* } = args;
    };
    let destruct = destruct.into();
    let stmt = syn::parse_macro_input!(destruct as syn::Stmt);

    func.block.stmts.insert(0, stmt);
  }

  let out = quote! {
    #arg_struct
    #func
  };
  out.into()
}
