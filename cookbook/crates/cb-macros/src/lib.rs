//! Procedural macros. Every entry point is a thin wrapper: parse the input
//! `TokenStream` with syn, build output with `quote!`, and turn any
//! `syn::Error` into `compile_error!` tokens so users get spanned errors
//! instead of a panic inside the compiler.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{
    Data, DeriveInput, Expr, Fields, GenericArgument, ItemFn, PathArguments, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
};

/// Turn `Result<TokenStream2, syn::Error>` into the TokenStream rustc wants.
fn finish(r: syn::Result<TokenStream2>) -> TokenStream {
    r.unwrap_or_else(|e| e.to_compile_error()).into()
}

// ---------------------------------------------------------------- Describe

/// `#[derive(Describe)]` generates `fn describe() -> &'static str`.
#[proc_macro_derive(Describe)]
pub fn derive_describe(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(describe_impl(input))
}

fn describe_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let body = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => {
                let parts: Vec<String> = f
                    .named
                    .iter()
                    .map(|f| format!("{}: {}", f.ident.as_ref().unwrap(), ty_str(&f.ty)))
                    .collect();
                format!("struct {name} {{ {} }}", parts.join(", "))
            }
            Fields::Unnamed(f) => {
                let parts: Vec<String> = f.unnamed.iter().map(|f| ty_str(&f.ty)).collect();
                format!("struct {name}({})", parts.join(", "))
            }
            Fields::Unit => format!("struct {name}"),
        },
        Data::Enum(e) => {
            let vs: Vec<String> = e.variants.iter().map(|v| v.ident.to_string()).collect();
            format!("enum {name} {{ {} }}", vs.join(", "))
        }
        Data::Union(u) => {
            return Err(syn::Error::new_spanned(
                u.union_token,
                "Describe cannot be derived for unions",
            ));
        }
    };
    // split_for_impl keeps generics, bounds and where clauses intact:
    // `impl<T: Bound> Name<T> where ... { ... }`.
    let (impl_g, ty_g, where_c) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_g #name #ty_g #where_c {
            pub fn describe() -> &'static str { #body }
        }
    })
}

/// Render a type without the odd spaces `to_token_stream` inserts ("Vec < u8 >").
fn ty_str(t: &Type) -> String {
    quote!(#t)
        .to_string()
        .replace(" < ", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace(" ,", ",")
}

// ----------------------------------------------------------------- Builder

/// `#[derive(Builder)]` for structs with named fields. `Option<T>` fields are
/// optional; every other field is required and reported by name in `build()`.
#[proc_macro_derive(Builder)]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(builder_impl(input))
}

/// If `ty` is `Option<X>`, return `X`.
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    match a.args.first()? {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    }
}

fn builder_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let builder = quote::format_ident!("{}Builder", name);
    let vis = &input.vis;
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            // Span the error on the fields so the squiggle lands on `(..)`.
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Builder requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                input.ident.span(),
                "Builder can only be derived for structs",
            ));
        }
    };
    let (impl_g, ty_g, where_c) = input.generics.split_for_impl();

    let mut decls = vec![];
    let mut setters = vec![];
    let mut inits = vec![];
    let mut defaults = vec![];
    for f in fields {
        let id = f.ident.as_ref().unwrap();
        let ty = &f.ty;
        // Every builder slot is an Option; the setter takes the *inner* type.
        let inner = option_inner(ty).unwrap_or(ty);
        decls.push(quote! { #id: ::core::option::Option<#inner> });
        defaults.push(quote! { #id: ::core::option::Option::None });
        setters.push(quote! {
            pub fn #id(mut self, v: #inner) -> Self { self.#id = ::core::option::Option::Some(v); self }
        });
        if option_inner(ty).is_some() {
            inits.push(quote! { #id: self.#id });
        } else {
            let msg = format!("missing required field `{id}`");
            inits.push(quote! {
                #id: self.#id.ok_or_else(|| ::std::string::String::from(#msg))?
            });
        }
    }
    Ok(quote! {
        #vis struct #builder #impl_g #where_c { #(#decls,)* }
        impl #impl_g ::core::default::Default for #builder #ty_g #where_c {
            fn default() -> Self { Self { #(#defaults,)* } }
        }
        impl #impl_g #builder #ty_g #where_c {
            #(#setters)*
            pub fn build(self) -> ::core::result::Result<#name #ty_g, ::std::string::String> {
                Ok(#name { #(#inits,)* })
            }
        }
        impl #impl_g #name #ty_g #where_c {
            pub fn builder() -> #builder #ty_g { ::core::default::Default::default() }
        }
    })
}

// ------------------------------------------------------------------- timed

/// `#[timed]` wraps the body, prints elapsed time to stderr, and keeps the
/// signature, visibility, attributes (incl. docs) and `async`-ness intact.
#[proc_macro_attribute]
pub fn timed(args: TokenStream, item: TokenStream) -> TokenStream {
    if !args.is_empty() {
        let a: TokenStream2 = args.into();
        return syn::Error::new_spanned(a, "#[timed] takes no arguments")
            .to_compile_error()
            .into();
    }
    let mut f = parse_macro_input!(item as ItemFn);
    let name = f.sig.ident.to_string();
    let block = &f.block;
    // For async fns we wrap in an async block so the timer spans the awaits.
    // The closure/async block preserves `return` semantics inside the body.
    let new_body: syn::Block = if f.sig.asyncness.is_some() {
        syn::parse_quote!({
            let __start = ::std::time::Instant::now();
            let __r = async move #block .await;
            ::std::eprintln!("[timed] {} took {:?}", #name, __start.elapsed());
            __r
        })
    } else {
        syn::parse_quote!({
            let __start = ::std::time::Instant::now();
            let __r = (move || #block)();
            ::std::eprintln!("[timed] {} took {:?}", #name, __start.elapsed());
            __r
        })
    };
    *f.block = new_body;
    quote!(#f).into()
}

// ---------------------------------------------------------------- memoize

/// `#[memoize]` caches results per argument tuple in a thread-local map.
/// Requires: plain `ident: Type` args that are `Clone + Hash + Eq`, and a
/// `Clone` return type. Only sensible for pure functions.
#[proc_macro_attribute]
pub fn memoize(_args: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);
    finish(memoize_impl(f))
}

fn memoize_impl(f: ItemFn) -> syn::Result<TokenStream2> {
    if f.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            f.sig.asyncness,
            "#[memoize] does not support async fn",
        ));
    }
    let syn::ReturnType::Type(_, ret) = &f.sig.output else {
        return Err(syn::Error::new_spanned(
            &f.sig,
            "#[memoize] needs a return type",
        ));
    };
    let mut names = vec![];
    let mut tys = vec![];
    for a in &f.sig.inputs {
        match a {
            syn::FnArg::Typed(pt) => match &*pt.pat {
                syn::Pat::Ident(i) => {
                    names.push(i.ident.clone());
                    tys.push((*pt.ty).clone());
                }
                other => {
                    return Err(syn::Error::new(
                        other.span(),
                        "#[memoize] needs simple identifier arguments",
                    ));
                }
            },
            syn::FnArg::Receiver(r) => {
                return Err(syn::Error::new_spanned(
                    r,
                    "#[memoize] does not support methods",
                ));
            }
        }
    }
    let attrs = &f.attrs;
    let vis = &f.vis;
    let sig = &f.sig;
    let block = &f.block;
    Ok(quote! {
        #(#attrs)*
        #vis #sig {
            ::std::thread_local! {
                static CACHE: ::std::cell::RefCell<::std::collections::HashMap<(#(#tys,)*), #ret>>
                    = ::std::cell::RefCell::new(::std::collections::HashMap::new());
            }
            let key = (#(::core::clone::Clone::clone(&#names),)*);
            // Do not hold the borrow while computing: the body may recurse
            // (fib!) and re-enter the cache.
            if let ::core::option::Option::Some(v) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
                return v;
            }
            let v: #ret = (move || #block)();
            CACHE.with(|c| c.borrow_mut().insert(key, ::core::clone::Clone::clone(&v)));
            v
        }
    })
}

// ---------------------------------------------------------------- hashmap!

struct Pairs(Vec<(Expr, Expr)>);
impl Parse for Pairs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut v = vec![];
        while !input.is_empty() {
            let k: Expr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let val: Expr = input.parse()?;
            v.push((k, val));
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(Pairs(v))
    }
}

/// `hashmap!{ "a" => 1, "b" => 2 }` (trailing comma allowed).
#[proc_macro]
pub fn hashmap(input: TokenStream) -> TokenStream {
    let Pairs(pairs) = parse_macro_input!(input as Pairs);
    let n = pairs.len();
    let (k, v): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    quote! {{
        let mut m = ::std::collections::HashMap::with_capacity(#n);
        #( m.insert(#k, #v); )*
        m
    }}
    .into()
}

// ------------------------------------------------------------ const_assert!

/// `const_assert!(size_of::<u32>() == 4, "message")`: fails the *build*.
/// The error is spanned on the condition expression.
#[proc_macro]
pub fn const_assert(input: TokenStream) -> TokenStream {
    struct A(Expr, Option<syn::LitStr>);
    impl Parse for A {
        fn parse(i: ParseStream) -> syn::Result<Self> {
            let e = i.parse()?;
            let m = if i.parse::<Token![,]>().is_ok() && !i.is_empty() {
                Some(i.parse()?)
            } else {
                None
            };
            Ok(A(e, m))
        }
    }
    let A(cond, msg) = parse_macro_input!(input as A);
    let text = msg
        .map(|m| m.value())
        .unwrap_or_else(|| "const assertion failed".into());
    // quote_spanned! makes the generated panic point at the user's condition.
    quote_spanned! { cond.span() =>
        const _: () = assert!(#cond, #text);
    }
    .into()
}
