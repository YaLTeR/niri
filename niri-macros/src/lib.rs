use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident};

#[proc_macro_derive(Mergeable, attributes(mergeable))]
pub fn derive_mergeable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let merge_body = match &input.data {
        Data::Struct(data) => {
            match &data.fields {
                Fields::Named(fields) => {
                    // Find all mutex fields
                    let mutex_fields: Vec<&Ident> = fields
                        .named
                        .iter()
                        .filter_map(|f| {
                            let is_mutex = f.attrs.iter().any(|attr| {
                                if attr.path().is_ident("mergeable") {
                                    // Parse the attribute contents
                                    let mut is_mutex_attr = false;
                                    let _ = attr.parse_nested_meta(|meta| {
                                        if meta.path.is_ident("mutex") {
                                            is_mutex_attr = true;
                                        }
                                        Ok(())
                                    });
                                    return is_mutex_attr;
                                }
                                false
                            });

                            if is_mutex {
                                f.ident.as_ref()
                            } else {
                                None
                            }
                        })
                        .collect();

                    // Generate merge code for each field
                    let field_merges = fields.named.iter().map(|f| {
                        let field_name = &f.ident;
                        let field_name_ref = field_name.as_ref().unwrap();

                        // Check if this field is marked as mutex
                        let is_mutex_field = mutex_fields.contains(&field_name_ref);

                        if is_mutex_field {
                            // Check if this field is a BoolFlag or plain bool
                            let field_type = &f.ty;
                            let is_bool_flag = match field_type {
                                syn::Type::Path(type_path) => type_path
                                    .path
                                    .segments
                                    .last()
                                    .map(|seg| seg.ident == "BoolFlag")
                                    .unwrap_or(false),
                                _ => false,
                            };

                            let is_bool = match field_type {
                                syn::Type::Path(type_path) => type_path
                                    .path
                                    .segments
                                    .last()
                                    .map(|seg| seg.ident == "bool")
                                    .unwrap_or(false),
                                _ => false,
                            };

                            if is_bool_flag {
                                quote! {
                                    self.#field_name.merge_with(&other.#field_name);
                                }
                            } else if is_bool {
                                if mutex_fields.len() == 1 {
                                    quote! {
                                        self.#field_name = other.#field_name;
                                    }
                                } else {
                                    let other_mutex_fields = mutex_fields
                                        .iter()
                                        .filter(|&&other_field| other_field != field_name_ref)
                                        .map(|&other_field| {
                                            quote! {
                                                self.#other_field = false;
                                            }
                                        });

                                    quote! {
                                        if other.#field_name {
                                            self.#field_name = true;
                                            #(#other_mutex_fields)*
                                        }
                                    }
                                }
                            } else {
                                return quote! {
                                    compile_error!(concat!("#[mergeable(mutex)] can only be used on bool or BoolFlag fields, but field '", stringify!(#field_name), "' has a different type"));
                                };
                            }
                        } else {
                            quote! {
                                self.#field_name.merge_with(&other.#field_name);
                            }
                        }
                    });

                    quote! {
                        #(#field_merges)*
                    }
                }
                Fields::Unnamed(fields) => {
                    let field_merges = (0..fields.unnamed.len()).map(|i| {
                        let idx = syn::Index::from(i);
                        quote! {
                            self.#idx.merge_with(&other.#idx);
                        }
                    });
                    quote! {
                        #(#field_merges)*
                    }
                }
                Fields::Unit => quote! {},
            }
        }
        Data::Enum(_) => {
            quote! {
                *self = other.clone();
            }
        }
        Data::Union(_) => panic!("Unions are not supported for Mergeable derive"),
    };

    let expanded = quote! {
        impl #impl_generics crate::mergeable::Mergeable for #name #ty_generics #where_clause {
            fn merge_with(&mut self, other: &Self) {
                #merge_body
            }
        }
    };

    TokenStream::from(expanded)
}
