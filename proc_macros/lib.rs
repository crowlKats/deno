extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, ToTokens};
use std::io::Write;
use syn::{FnArg, ItemFn, Pat, PatType};

fn get_args(func: &ItemFn) -> (Option<PatType>, Vec<PatType>, Option<PatType>) {
  let is_async = func.sig.asyncness.is_some();

  let mut op_state: Option<PatType> = None;
  let mut args: Vec<PatType> = vec![];
  let mut zero_copy: Option<PatType> = None;

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

  (op_state, args, zero_copy)
}

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
        if !s.is_empty() {
          let mut chars: Vec<char> = s.chars().collect();
          chars[0] = chars[0].to_uppercase().next().unwrap();
          chars.into_iter().collect::<String>()
        } else {
          String::new()
        }
      })
      .collect::<Vec<_>>()
      .join("");
    Ident::new(&name, ident.span())
  };

  let (op_state, args, zero_copy) = get_args(&func);

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
      #[derive(Deserialize)]
      #[serde(rename_all = "camelCase")]
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

#[proc_macro_attribute]
pub fn deno_bindgen(_: TokenStream, item: TokenStream) -> TokenStream {
  let func = syn::parse_macro_input!(item as syn::ItemFn);
  let is_async = func.sig.asyncness.is_some();
  let func_name = func.sig.ident.to_string();
  let js_func_name = func_name.clone().split_off(3);
  let (_, args, zero_copy) = get_args(&func);
  let item = deno_op(Default::default(), func.into_token_stream().into());

  let args = args
    .into_iter()
    .map(|pat| match &*pat.pat {
      Pat::Ident(ident) => ident.ident.to_string(),
      _ => unreachable!(),
    })
    .collect::<Vec<_>>();
  let str_args = if args.is_empty() {
    String::new()
  } else {
    if args.len() == 1 {
      args[0].clone()
    } else {
      args.join(", ")
    }
  };
  let buffer = if zero_copy.is_some() { "buffer" } else { "" };
  let arguments = format!(
    "{}",
    if !args.is_empty() {
      if zero_copy.is_some() {
        format!("{}, {}", str_args, buffer)
      } else {
        str_args.clone()
      }
    } else {
      buffer.to_string()
    }
  );
  let obj_arguments = if args.is_empty() {
    "{}".to_string()
  } else {
    if args.len() == 1 {
      str_args
    } else {
      format!("{{ {} }}", str_args)
    }
  };
  let dispatch_arguments = format!(
    "{}",
    if zero_copy.is_some() {
      format!("{}, {}", obj_arguments, buffer)
    } else {
      obj_arguments
    }
  );

  let mut file = std::fs::OpenOptions::new()
    .append(true)
    .create(true)
    .open("./bindgen.js")
    .expect("open bindgen file");

  writeln!(
    file,
    r#"export function {}({}) {{ return Deno.core.json{}("{}", {}); }}"#,
    js_func_name,
    arguments,
    if is_async { "Async" } else { "Sync" },
    func_name,
    dispatch_arguments,
  )
  .expect("write to bindgen file");

  item
}
