use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::{ast::App };

use crate::{codegen::util, analyze::Analysis};

/// Generates support code for the KLEE test harness
pub fn codegen(app: &App, analysis: &Analysis) -> Vec<TokenStream2> {
    let app_name = &app.name;
    let app_path = quote! {crate::#app_name};
    
    let mut test_harness = vec![];
    let mut task_list = vec![];
    let mut match_stmts = vec![];
    let mut task_number: u8 = 0;
    
    // Add init function
    // let init_name = &app.inits.first().unwrap().name;
    // task_list.push(quote!(
    //    #task_number => {
    //        let mut core: rtic::export::Peripherals =
    //            rtic::export::Peripherals::steal().into();
    //        #app_path::#init_name(#init_name::Context::new(core.into()));
    //    }
    // ));
    // task_number += 1;

    // Fetch all tasks for KLEE to match
    for (name, task) in &app.hardware_tasks {
        let mut resources = vec![];
        for (name, _access) in &task.args.resources {            
            let mangled_name = util::mark_internal_ident(&name);
            let name_as_str: String = mangled_name.to_string();
            let (res, _expr) = app.resource(name).expect("UNREACHABLE");
            let ty = &res.ty;
            let mangled_name_ty_as_str: String = mangled_name.to_string() + "_type";
            let mangled_name_ty = util::suffixed(&mangled_name_ty_as_str);

            if app.late_resources.contains_key(name) {
                resources.push(quote!(
                    /// Check if LateResource is supported
                    if late_type_supported(&#mangled_name, &supported_late_types) {
                        klee_make_symbolic!(&mut #mangled_name, #name_as_str);
                    }
                ));
            } else {
                resources.push(quote!(
                    klee_make_symbolic!(&mut #mangled_name, #name_as_str);
                ));
            }
        }
        task_list.push(quote!(
            #task_number => {
                #(#resources)*
                #app_path::#name(#name::Context::new(&rtic::export::Priority::new(1)));
            }
        ));
        task_number += 1;
    }
    
    for (name, task) in &app.software_tasks{
        let mut resources = vec![];
        for (name, _access) in &task.args.resources {
            let mangled_name = util::mark_internal_ident(&name);
            let name_as_str: String = mangled_name.to_string();

            // Check if type is in supported types
            resources.push(quote!(
                klee_make_symbolic!(&mut #mangled_name, #name_as_str);
            ));
        }
        task_list.push(quote!(
            #task_number => {
                #(#resources)*   
                #app_path::#name(#name::Context::new(&rtic::export::Priority::new(1)));
            }
        ));
        task_number += 1;
    }
    
    // Insert all tasks inside a match
    match_stmts.push(quote!(
        match __klee_task_id {
            #(#task_list)*
            _ => ()
        }
    ));
    
    // Finish test harness
    test_harness.push(quote!(
        klee_make_symbolic!(&mut __klee_task_id, "__klee_task_id");
        #(#match_stmts)*
    ));
    test_harness
} 