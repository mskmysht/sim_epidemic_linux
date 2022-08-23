extern crate proc_macro;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Error};

#[proc_macro_derive(Enum)]
pub fn enum_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as DeriveInput);
    let name = item.ident;
    let gen = match item.data {
        Data::Enum(data_enum) => {
            let res =
                data_enum
                    .variants
                    .iter()
                    .map(|v| match v.fields {
                        syn::Fields::Unit => Ok(&v.ident),
                        _ => Err(Error::new(v.ident.span(), "support unit field only")
                            .to_compile_error()),
                    })
                    .collect::<Result<Vec<_>, _>>();
            match res {
                Ok(vnames) => {
                    let num = data_enum.variants.len();
                    let counter = 0..num;

                    let usize_ident = {
                        let counter = counter.clone();
                        quote! {
                            #(
                                #counter => #name::#vnames,
                            )*
                        }
                    };
                    let ident_usize = quote! {
                        #(
                            #name::#vnames => #counter,
                        )*
                    };

                    quote! {
                    #[automatically_derived]
                    impl Enum for #name {
                        type Arr = [Self; #num];
                        const LEN: usize = #num;
                        const ALL: Self::Arr = [
                            #(
                                #name::#vnames,
                            )*
                        ];

                        // fn from_usize(u: usize) -> Self {
                        //     match u {
                        //         #usize_ident
                        //         _ => unreachable!(),
                        //     }
                        // }

                        fn to_index(&self) -> usize {
                            match self {
                                #ident_usize
                            }
                        }
                    }
                    }
                }
                Err(e) => e,
            }
        }
        _ => Error::new(name.span(), "For enum only").to_compile_error(),
    };
    gen.into()
}
