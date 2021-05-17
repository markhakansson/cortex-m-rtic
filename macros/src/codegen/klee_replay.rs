use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::{ast::App };

use crate::{analyze::Analysis, codegen::util};

/// Generates support code for the KLEE replay harness
pub fn codegen(app: &App, analysis: &Analysis) -> Vec<TokenStream2> {
    let app_name = &app.name;
    let app_path = quote! {crate::#app_name};
    
    let mut test_harness = vec![];
    let mut task_list = vec![];
    let mut match_stmts = vec![];
    let mut task_number: u8 = 0;
    
    // Hardware tasks. Just call the correct symbol.
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


    // Software tasks. Call the correct interrupt which handles the software task.
    for (task_name, task) in &app.software_tasks{
        let priority = task.args.priority;
        let t = util::spawn_t_ident(priority);
        let interrupt = &analysis
        .interrupts
        .get(&priority)
        .expect("RTIC-ICE: interrupt identifer not found")
        .0;
        let fq = util::fq_ident(task_name);
        let fq = util::mark_internal_ident(&fq);
        let rq = util::rq_ident(priority);
        let rq = util::mark_internal_ident(&rq);

        let doc = format!("{}", task_name);

        task_list.push(quote!(
            #task_number => {
                #[doc = #doc]
                // Push task to queue
                if let Some(index) = #app_path::#fq.get_mut_unchecked().dequeue() {
                    // Enqueue the task
                    #app_path::#rq.get_mut_unchecked().enqueue_unchecked((#app_path::#t::#task_name, index));
                    // Call interrupt directly
                    #interrupt();
                }
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