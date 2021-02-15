use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::{ast::App };

use crate::{codegen::util, analyze::Analysis};

pub fn codegen(app: &App, analysis: &Analysis) -> Vec<TokenStream2> {
    let app_name = &app.name;
    let app_path = quote! {crate::#app_name};
    
    let mut res = vec![];
    let mut task_list = vec![];
    let mut match_stmts = vec![];
    let mut resource_list = vec![];
    let mut task_number: u32= 0;
    
    res.push(quote!(
        let mut task = 0;
        klee_make_symbolic!(&mut task, "task");
    ));

    for (name, resource, expr, _) in app.resources(analysis){
        let ty = &resource.ty;
        let mangled_name = util::mangle_ident(&name);
        {
            resource_list.push(quote!(
                let mut #name = 0;
                klee_make_symbolic!(&mut #name, "#name");
            ));

            if let Some(expr) = expr {
                resource_list.push(quote!(
                    #mangled_name = #name;    
                ));
            } else {
                resource_list.push(quote!(
                    #mangled_name.as_mut_ptr().write(#name);
                ));
            };
        }
    }
    for (name, _task) in &app.hardware_tasks {
        task_list.push(quote!(
            #task_number => #app_path::#name(#name::Context::new(&rtic::export::Priority::new(1))),
        ));
        task_number += 1;
    }
    for (name, _task) in &app.software_tasks{
        task_list.push(quote!(
            #task_number => #app_path::#name(#name::Context::new(&rtic::export::Priority::new(1))),
        ));
        task_number += 1;
    }
    match_stmts.push(quote!(
        match task {
            #(#task_list)*
            _ => ()
        }
    ));
    
    res.append(&mut resource_list);
    res.append(&mut match_stmts);
    res
} 