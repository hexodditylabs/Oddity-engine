//! `#[script]`: turns a struct into an Oddity script (`roadmap/scripting.md`,
//! "`#[script]`"). Games reach it as `oddity::prelude::script`.
//!
//! ```ignore
//! #[script(id = "4f9d2c71", order = -10)]
//! pub struct Health {
//!     #[default(100.0)]
//!     pub max: f32,
//!     current: f32,
//! }
//! ```
//!
//! becomes a Bevy `Component` with `Reflect` (so it registers itself), a
//! `Default` built from the `#[default(..)]` values, and `ReflectScript` type
//! data carrying its `id` and `order`. Saved fields (`pub`, or `#[show]`,
//! unless `#[hide]`) get the `Export` attribute; the Inspector attributes
//! (`range`, `min`, `tooltip`, `header`, `multiline`, and `///` docs as the
//! tooltip) become reflect custom attributes.
//!
//! On an `impl` block, `#[script]` collects the `#[listener]` methods into
//! the type's listener table, which events and `connect` look names up in:
//!
//! ```ignore
//! #[script]
//! impl Bullet {
//!     #[listener]
//!     fn expire(&mut self, ctx: &mut Ctx) {
//!         ctx.destroy();
//!     }
//! }
//! ```
//!
//! The module using it needs `use oddity::prelude::*;`. Bevy's derives name
//! `bevy_ecs` and `bevy_reflect` relative to the call site when the game's
//! `Cargo.toml` doesn't list `bevy`, and the prelude provides both names.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, quote};
use syn::{
    Attribute, Expr, ExprLit, Fields, FnArg, Ident, ImplItem, Item, ItemImpl, ItemStruct, Lit,
    LitStr, Meta, MetaNameValue, Path, Token, Type, Visibility, parse::Parser, parse_macro_input,
    punctuated::Punctuated, spanned::Spanned,
};

/// Make a struct an Oddity script, or collect a script's `#[listener]`
/// methods from its impl block. See the crate docs.
#[proc_macro_attribute]
pub fn script(args: TokenStream, item: TokenStream) -> TokenStream {
    let args =
        parse_macro_input!(args with Punctuated::<MetaNameValue, Token![,]>::parse_terminated);
    let item = parse_macro_input!(item as Item);
    let expanded = match item {
        Item::Struct(item) => expand(args, item),
        Item::Impl(item) => expand_impl(args, item),
        other => Err(syn::Error::new(
            other.span(),
            "#[script] goes on a struct, or on the impl block of a script's listeners",
        )),
    };
    expanded
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Marks a method as a listener events can call. Only meaningful inside a
/// `#[script] impl` block, which removes it; anywhere else it is an error.
#[proc_macro_attribute]
pub fn listener(_args: TokenStream, item: TokenStream) -> TokenStream {
    outside_script_impl(item, "#[listener]")
}

/// Marks a method as the handler of a signal: `#[on(EnemyKilled)]`. Only
/// meaningful inside a `#[script] impl` block, which removes it; anywhere
/// else it is an error.
#[proc_macro_attribute]
pub fn on(_args: TokenStream, item: TokenStream) -> TokenStream {
    outside_script_impl(item, "#[on(..)]")
}

fn outside_script_impl(item: TokenStream, attribute: &str) -> TokenStream {
    let item = TokenStream2::from(item);
    let error = syn::Error::new(
        Span::call_site(),
        format!(
            "{attribute} methods need #[script] on their impl block: `#[script] impl Name {{ .. }}`"
        ),
    )
    .into_compile_error();
    quote!(#error #item).into()
}

/// Make a struct a game-wide signal: a Bevy `Message` systems can read,
/// which `ctx.emit` sends and `#[on(..)]` handlers receive.
#[proc_macro_attribute]
pub fn signal(args: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemStruct);
    if !args.is_empty() {
        return syn::Error::new(Span::call_site(), "#[signal] takes no settings")
            .into_compile_error()
            .into();
    }
    if !item.generics.params.is_empty() {
        return syn::Error::new(
            item.generics.span(),
            "a signal can't have generic parameters",
        )
        .into_compile_error()
        .into();
    }
    let runtime = runtime();
    let private = quote!(#runtime::__private);
    let signal_path: Path = syn::parse_quote!(#runtime::Signal);
    let name = &item.ident;
    quote! {
        #[derive(::core::clone::Clone, #private::Message, #private::Reflect)]
        #[reflect(#signal_path)]
        #item

        impl #runtime::Signal for #name {}
    }
    .into()
}

/// `#[script]` on an impl block: build the tables of its `#[listener]` and
/// `#[on(..)]` methods, as inherent constants the struct's `#[script]`
/// picks up.
fn expand_impl(
    args: Punctuated<MetaNameValue, Token![,]>,
    mut item: ItemImpl,
) -> syn::Result<TokenStream2> {
    if let Some(arg) = args.first() {
        return Err(syn::Error::new(
            arg.span(),
            "#[script] on an impl block takes no settings; they go on the struct",
        ));
    }
    if let Some((_, path, _)) = &item.trait_ {
        return Err(syn::Error::new(
            path.span(),
            "#[script] goes on the script's own `impl Name { .. }`, not on a trait impl",
        ));
    }
    let runtime = runtime();
    let private = quote!(#runtime::__private);
    let mut listeners = Vec::new();
    let mut handlers = Vec::new();
    for impl_item in &mut item.items {
        let ImplItem::Fn(method) = impl_item else {
            continue;
        };
        let mut is_listener = false;
        let mut signals = Vec::new();
        let mut kept = Vec::new();
        for attr in method.attrs.drain(..) {
            if attr.path().is_ident("listener") {
                is_listener = true;
            } else if attr.path().is_ident("on") {
                signals.push(attr.parse_args::<Type>()?);
            } else {
                kept.push(attr);
            }
        }
        method.attrs = kept;
        if !is_listener && signals.is_empty() {
            continue;
        }
        let inputs: Vec<&FnArg> = method.sig.inputs.iter().collect();
        let takes_mut_self = matches!(inputs.first(), Some(FnArg::Receiver(receiver)) if receiver.mutability.is_some());
        let name = &method.sig.ident;
        if is_listener {
            listeners.push(listener_entry(
                &private,
                name,
                &inputs,
                takes_mut_self,
                method.sig.span(),
            )?);
        }
        for signal in signals {
            if !takes_mut_self || inputs.len() != 3 {
                return Err(syn::Error::new(
                    method.sig.span(),
                    "a signal handler takes (&mut self, ctx: &mut Ctx, signal: &TheSignal)",
                ));
            }
            handlers.push(quote! {
                #private::HandlerEntry {
                    signal: || ::core::any::TypeId::of::<#signal>(),
                    call: |this, ctx, signal| {
                        if let ::core::option::Option::Some(signal) = signal.downcast_ref::<#signal>() {
                            Self::#name(this, ctx, signal);
                        }
                    },
                }
            });
        }
    }
    item.items.push(syn::parse_quote! {
        #[doc(hidden)]
        const __ODDITY_LISTENERS: &'static [#private::ListenerEntry<Self>] = &[#(#listeners),*];
    });
    item.items.push(syn::parse_quote! {
        #[doc(hidden)]
        const __ODDITY_HANDLERS: &'static [#private::HandlerEntry<Self>] = &[#(#handlers),*];
    });
    Ok(quote!(#item))
}

/// One `#[listener]` method's entry in the listener table.
fn listener_entry(
    private: &TokenStream2,
    name: &Ident,
    inputs: &[&FnArg],
    takes_mut_self: bool,
    span: Span,
) -> syn::Result<TokenStream2> {
    let shape = || {
        syn::Error::new(
            span,
            "a listener takes (&mut self, ctx: &mut Ctx) or (&mut self, ctx: &mut Ctx, value: &T)",
        )
    };
    if !takes_mut_self {
        return Err(shape());
    }
    let label = LitStr::new(&name.to_string(), name.span());
    match inputs.len() {
        2 => Ok(quote! {
            #private::ListenerEntry {
                name: #label,
                arg: ::core::option::Option::None,
                call: |this, ctx, _value| Self::#name(this, ctx),
            }
        }),
        3 => {
            let FnArg::Typed(value) = inputs[2] else {
                return Err(shape());
            };
            let Type::Reference(reference) = &*value.ty else {
                return Err(syn::Error::new(
                    value.ty.span(),
                    "a listener's value is a reference: `value: &f32`",
                ));
            };
            let value_type = &reference.elem;
            Ok(quote! {
                #private::ListenerEntry {
                    name: #label,
                    arg: ::core::option::Option::Some(
                        <#value_type as #private::TypePath>::type_path,
                    ),
                    call: |this, ctx, value| {
                        if let ::core::option::Option::Some(value) =
                            value.and_then(|value| value.try_downcast_ref::<#value_type>())
                        {
                            Self::#name(this, ctx, value);
                        }
                    },
                }
            })
        }
        _ => Err(shape()),
    }
}

/// Where the runtime lives as seen from the crate being compiled: `oddity`
/// for games, `oddity_script` for its own tests. No leading `::`: Bevy's
/// `#[reflect(..)]` only takes paths that start with a name.
fn runtime() -> TokenStream2 {
    if std::env::var("CARGO_PKG_NAME").as_deref() == Ok("oddity_script") {
        quote!(oddity_script)
    } else {
        quote!(oddity)
    }
}

/// The `id` and `order` given to `#[script(..)]`.
struct ScriptArgs {
    id: Option<LitStr>,
    order: Option<Expr>,
}

fn parse_args(args: Punctuated<MetaNameValue, Token![,]>) -> syn::Result<ScriptArgs> {
    let mut parsed = ScriptArgs {
        id: None,
        order: None,
    };
    for arg in args {
        if arg.path.is_ident("id") {
            match arg.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(id), ..
                }) => parsed.id = Some(id),
                other => return Err(syn::Error::new(other.span(), "`id` is a string")),
            }
        } else if arg.path.is_ident("order") {
            parsed.order = Some(arg.value);
        } else {
            return Err(syn::Error::new(
                arg.path.span(),
                "unknown setting; #[script] takes `id` and `order`",
            ));
        }
    }
    Ok(parsed)
}

/// What a field's attributes say about it.
#[derive(Default)]
struct FieldSettings {
    default: Option<Expr>,
    show: bool,
    hide: bool,
    custom: Vec<TokenStream2>,
    tooltip: Option<TokenStream2>,
    docs: Vec<String>,
}

/// Take our attributes off a field, leaving the rest (docs, other
/// derives' attributes) in place.
fn take_settings(attrs: &mut Vec<Attribute>, runtime: &TokenStream2) -> syn::Result<FieldSettings> {
    let mut settings = FieldSettings::default();
    let attributes = quote!(#runtime::attributes);
    let mut kept = Vec::new();
    for attr in attrs.drain(..) {
        let path = attr.path();
        let name = path.get_ident().map(Ident::to_string);
        match name.as_deref() {
            Some("default") => settings.default = Some(attr.parse_args()?),
            Some("show") => settings.show = true,
            Some("hide") => settings.hide = true,
            Some("multiline") => settings.custom.push(quote!(#attributes::Multiline)),
            Some("header") => {
                let text: LitStr = attr.parse_args()?;
                settings.custom.push(quote!(#attributes::Header(#text)));
            }
            Some("tooltip") => {
                let text: LitStr = attr.parse_args()?;
                settings.tooltip = Some(quote!(#attributes::Tooltip(#text)));
            }
            Some("min") => {
                let min: Expr = attr.parse_args()?;
                settings
                    .custom
                    .push(quote!(#attributes::Min((#min) as f64)));
            }
            Some("range") => {
                let bounds = Punctuated::<Expr, Token![,]>::parse_terminated
                    .parse2(attr.meta.require_list()?.tokens.clone())?;
                let [min, max] = bounds.iter().collect::<Vec<_>>()[..] else {
                    return Err(syn::Error::new(
                        attr.span(),
                        "#[range] takes two values: #[range(0.0, 10.0)]",
                    ));
                };
                settings
                    .custom
                    .push(quote!(#attributes::Range { min: (#min) as f64, max: (#max) as f64 }));
            }
            Some("doc") => {
                if let Meta::NameValue(MetaNameValue {
                    value:
                        Expr::Lit(ExprLit {
                            lit: Lit::Str(line),
                            ..
                        }),
                    ..
                }) = &attr.meta
                {
                    settings.docs.push(line.value().trim().to_owned());
                }
                kept.push(attr);
            }
            _ => kept.push(attr),
        }
    }
    *attrs = kept;
    Ok(settings)
}

fn expand(
    args: Punctuated<MetaNameValue, Token![,]>,
    mut item: ItemStruct,
) -> syn::Result<TokenStream2> {
    let args = parse_args(args)?;
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(
            item.generics.span(),
            "a script can't have generic parameters",
        ));
    }
    let runtime = runtime();
    let private = quote!(#runtime::__private);
    let name = &item.ident;

    let mut defaults = Vec::new();
    match &mut item.fields {
        Fields::Named(fields) => {
            for field in &mut fields.named {
                let settings = take_settings(&mut field.attrs, &runtime)?;
                let ident = field.ident.clone().expect("named fields have names");
                let value = settings.default.map_or_else(
                    || quote!(::core::default::Default::default()),
                    |e| e.to_token_stream(),
                );
                defaults.push(quote!(#ident: #value));

                let public = matches!(field.vis, Visibility::Public(_));
                let saved = (public || settings.show) && !settings.hide;
                let tooltip = settings.tooltip.or_else(|| {
                    let text = settings.docs.join("\n");
                    let text = text.trim();
                    (!text.is_empty()).then(|| {
                        let text = LitStr::new(text, Span::call_site());
                        quote!(#runtime::attributes::Tooltip(#text))
                    })
                });
                let mut attributes = settings.custom;
                attributes.extend(tooltip);
                if saved {
                    attributes.push(quote!(#runtime::attributes::Export));
                }
                for attribute in attributes {
                    field.attrs.push(syn::parse_quote!(#[reflect(@#attribute)]));
                }
            }
        }
        Fields::Unit => {}
        Fields::Unnamed(fields) => {
            return Err(syn::Error::new(
                fields.span(),
                "a script has named fields, or none: `pub struct Tag;`",
            ));
        }
    }

    let default_body = match &item.fields {
        Fields::Unit => quote!(Self),
        _ => quote!(Self { #(#defaults),* }),
    };
    let id = match &args.id {
        Some(id) => quote!(::core::option::Option::Some(#id)),
        None => quote!(::core::option::Option::None),
    };
    let order = args
        .order
        .map_or_else(|| quote!(0), |order| order.to_token_stream());
    let script_path: Path = syn::parse_quote!(#runtime::Script);

    Ok(quote! {
        #[derive(#private::Component, #private::Reflect)]
        #[component(on_add = #private::script_added, on_remove = #private::script_removed)]
        #[reflect(
            #private::ReflectComponent,
            #private::ReflectDefault,
            #script_path
        )]
        #item

        impl ::core::default::Default for #name {
            fn default() -> Self {
                #default_body
            }
        }

        impl #private::ScriptMeta for #name {
            const ID: ::core::option::Option<&'static str> = #id;
            const ORDER: i32 = #order;
            const FILE: &'static str = ::core::file!();
            const CRATE_DIR: &'static str = ::core::env!("CARGO_MANIFEST_DIR");
            // The tables `#[script] impl` builds are inherent constants, which
            // take precedence over the empty ones `EmptyTables` gives every
            // type.
            const LISTENERS: &'static [#private::ListenerEntry<Self>] = {
                #[allow(unused_imports)]
                use #private::EmptyTables as _;
                #name::__ODDITY_LISTENERS
            };
            const HANDLERS: &'static [#private::HandlerEntry<Self>] = {
                #[allow(unused_imports)]
                use #private::EmptyTables as _;
                #name::__ODDITY_HANDLERS
            };
        }
    })
}
