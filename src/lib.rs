use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Data, attributes(data))]
pub fn derive_data(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    impl_derive_data(ast)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

fn impl_derive_data(ast: DeriveInput) -> syn::Result<TokenStream> {
    let name = &ast.ident;

    let (values, links) = match ast.data {
        syn::Data::Struct(s) => gen_impls_for_struct(s)?,
        syn::Data::Enum(_e) => todo!(),
        syn::Data::Union(_) => todo!(),
    };

    Ok(quote! {
        impl ::datalink::data::Data for #name {
            #[allow(unused_variables)]
            #[inline]
            fn provide_value(&self, mut request: ::datalink::value::ValueRequest) {
                use ::datalink::value::Provided as _;
                self.provide_requested(&mut request).debug_assert_provided();
            }

            #[allow(unused_variables)]
            #[inline]
            fn provide_requested<R: ::datalink::value::Req>(&self, request: &mut ::datalink::value::ValueRequest<R>) -> impl ::datalink::value::Provided {
                #values
            }

            #[allow(unused_variables)]
            fn provide_links(&self, links: &mut dyn ::datalink::links::Links) -> Result<(),::datalink::links::LinkError> {
                use ::datalink::links::LinksExt as _;
                #links
                Ok(())
            }
        }
    })
}

fn gen_impls_for_struct(
    data: syn::DataStruct,
) -> syn::Result<(impl quote::ToTokens, impl quote::ToTokens)> {
    let mut values = Vec::new();
    let mut links = Vec::new();

    for (field_index, field) in data.fields.into_iter().enumerate() {
        let mut options = FieldOptions::default();

        let default_key: syn::Expr = match &field.ident {
            Some(ident) => {
                let ident = ident.to_string();
                syn::parse_quote!(#ident)
            }
            None => syn::parse_quote!(#field_index),
        };
        let default_target: syn::Expr = match &field.ident {
            Some(ident) => syn::parse_quote!(self.#ident.to_owned()),
            None => {
                let index = syn::Index::from(field_index);
                syn::parse_quote!(self.#index.to_owned())
            }
        };

        for attr in field.attrs {
            // This attribute is not for us
            if !attr.path().is_ident("data") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    options.skip = true;
                    return Ok(());
                }
                if meta.path.is_ident("link") {
                    let mut key = default_key.clone();
                    let mut target = default_target.clone();
                    let mut id = None;
                    meta.parse_nested_meta(|meta| {
                        if meta.path.is_ident("key") {
                            key = meta.value()?.parse()?;
                            return Ok(());
                        }
                        if meta.path.is_ident("target") {
                            target = meta.value()?.parse()?;
                            return Ok(());
                        }
                        if meta.path.is_ident("id") {
                            id.replace(meta.value()?.parse()?);
                            return Ok(());
                        }

                        Err(meta.error("unsupported property"))
                    })?;
                    options.links.push(LinkOptions { key, target, id });
                    return Ok(());
                }
                if meta.path.is_ident("value") {
                    // let ty: syn::Type = meta.value()?.parse()?;
                    options.value = true;
                    return Ok(());
                }
                Err(meta.error("unsupported property"))
            })?;
        }

        if options.skip {
            continue;
        }

        if options.value {
            let ident = if let Some(ident) = field.ident {
                quote!(self.#ident)
            } else {
                let index = syn::Index::from(field_index);
                quote!(self.#index)
            };
            values.push(quote! {
                request.provide_ref(&#ident);
            });
        }

        if options.links.is_empty() {
            options.links.push(LinkOptions {
                key: default_key,
                target: default_target,
                id: None,
            });
        }

        for link in options.links.drain(..) {
            let LinkOptions { key, target, id } = link;
            if id.is_some() {
                // unimplemented!("id")
            }
            links.push((key, target));
        }
    }

    Ok((
        gen_provide_values_impl(values),
        gen_provide_links_impl(links),
    ))
}

fn gen_provide_values_impl(
    values: impl IntoIterator<Item = impl quote::ToTokens>,
) -> impl quote::ToTokens {
    values
        .into_iter()
        .map(|value| {
            quote! {
                #value
            }
        })
        .collect::<proc_macro2::TokenStream>()
}

fn gen_provide_links_impl(
    links: impl IntoIterator<Item = (impl quote::ToTokens, impl quote::ToTokens)>,
) -> impl quote::ToTokens {
    links
        .into_iter()
        .map(|(key, target)| {
            quote! {
                links.push_link((#key, #target)).unwrap();
            }
        })
        .collect::<proc_macro2::TokenStream>()
}

struct LinkOptions {
    key: syn::Expr,
    target: syn::Expr,
    id: Option<syn::Expr>,
}

#[derive(Default)]
struct FieldOptions {
    skip: bool,
    links: Vec<LinkOptions>,
    value: bool,
}
