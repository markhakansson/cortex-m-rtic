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
    let mut symbolic_resources= vec![];
    let mut task_number: u32= 0;

    // Make resources symbolic
    for (name, _, _, _) in app.resources(analysis){
        let mangled_name = util::mangle_ident(&name);
        
        let name_as_str: String = app_name.to_string() + "::resources::"+ &name.to_string();

        // Does only work for core types
        symbolic_resources.push(quote!(
            klee_make_symbolic!(&mut #mangled_name, #name_as_str);
        ));
    }
    
    // Add init function
    let init_name = &app.inits.first().unwrap().name;
    task_list.push(quote!(
        #task_number => {
            let mut core: rtic::export::Peripherals =
                rtic::export::Peripherals::steal().into();
            #app_path::#init_name(#init_name::Context::new(core.into()));
        },
    ));
    task_number += 1;

    // Fetch all tasks for KLEE to match
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
    
    // Insert all tasks inside a match
    match_stmts.push(quote!(
        match task_id {
            #(#task_list)*
            _ => ()
        }
    ));
    
    // Finish test harness
    test_harness.push(quote!(
        let mut task_id = 0;
        klee_make_symbolic!(&mut task_id, "__klee_task_id");
        #(#symbolic_resources)*
        #(#match_stmts)*
    ));
    test_harness
} 