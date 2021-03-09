use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::{ast::App };

use crate::analyze::Analysis;

/// Generates support code for the KLEE replay harness
pub fn codegen(app: &App, _analysis: &Analysis) -> Vec<TokenStream2> {
    let app_name = &app.name;
    let app_path = quote! {crate::#app_name};
    
    let mut test_harness = vec![];
    let mut task_list = vec![];
    let mut match_stmts = vec![];
    let mut task_number: u32= 0;
    
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
    for (name, task) in &app.hardware_tasks {
        let symbol = task.args.binds.clone();
        let doc = format!("{}", name);

        task_list.push(quote!(
            #task_number => {
                #[doc = #doc]
                #symbol();
            }
        ));
        task_number += 1;
    }
    
    for (name, _task) in &app.software_tasks{
        task_list.push(quote!(
            #task_number => {
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
        #(#match_stmts)*
    ));
    test_harness
} 