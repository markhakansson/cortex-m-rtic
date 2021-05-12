use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::ast::App;

use crate::{analyze::Analysis, check::Extra};

mod assertions;
mod dispatchers;
mod hardware_tasks;
mod idle;
mod init;
mod locals;
mod module;
mod post_init;
mod pre_init;
mod resources;
mod resources_struct;
mod software_tasks;
mod timer_queue;
mod util;

#[cfg(feature = "klee-analysis")]   
mod klee;
#[cfg(feature = "klee-replay")]
mod klee_replay;

#[cfg(not(any(feature = "klee-analysis", feature = "klee-replay")))]
// TODO document the syntax here or in `rtic-syntax`
pub fn app(app: &App, analysis: &Analysis, extra: &Extra) -> TokenStream2 {
    let mut mod_app = vec![];
    let mut mains = vec![];
    let mut root = vec![];
    let mut user = vec![];

    // Generate the `main` function
    let assertion_stmts = assertions::codegen(app, analysis);

    let pre_init_stmts = pre_init::codegen(app, analysis, extra);

    let (mod_app_init, root_init, user_init, call_init) = init::codegen(app, analysis, extra);

    let post_init_stmts = post_init::codegen(app, analysis);

    let (mod_app_idle, root_idle, user_idle, call_idle) = idle::codegen(app, analysis, extra);

    user.push(quote!(
        #user_init

        #user_idle
    ));

    root.push(quote!(
        #(#root_init)*

        #(#root_idle)*
    ));

    mod_app.push(quote!(
        #mod_app_init

        #mod_app_idle
    ));

    let main = util::suffixed("main");
    mains.push(quote!(
        #[doc(hidden)]
        mod rtic_ext {
            use super::*;
            #[no_mangle]
            unsafe extern "C" fn #main() -> ! {
                #(#assertion_stmts)*

                #(#pre_init_stmts)*

                #[inline(never)]
                fn __rtic_init_resources<F>(f: F) where F: FnOnce() {
                    f();
                }

                // Wrap late_init_stmts in a function to ensure that stack space is reclaimed.
                __rtic_init_resources(||{
                    #call_init

                    #(#post_init_stmts)*
                });

                #call_idle
            }
        }
    ));

    let (mod_app_resources, mod_resources) = resources::codegen(app, analysis, extra);

    let (mod_app_hardware_tasks, root_hardware_tasks, user_hardware_tasks) =
        hardware_tasks::codegen(app, analysis, extra);

    let (mod_app_software_tasks, root_software_tasks, user_software_tasks) =
        software_tasks::codegen(app, analysis, extra);

    let mod_app_dispatchers = dispatchers::codegen(app, analysis, extra);
    let mod_app_timer_queue = timer_queue::codegen(app, analysis, extra);
    let user_imports = &app.user_imports;
    let user_code = &app.user_code;
    let name = &app.name;
    let device = &extra.device;
    let app_name = &app.name;
    let app_path = quote! {crate::#app_name};

    let monotonic_parts: Vec<_> = app
        .monotonics
        .iter()
        .map(|(_, monotonic)| {
            let name = &monotonic.ident;
            let name_str = &name.to_string();
            let ty = &monotonic.ty;
            let ident = util::monotonic_ident(&name_str);
            let ident = util::mark_internal_ident(&ident);
            let panic_str = &format!(
                "Use of monotonic '{}' before it was passed to the runtime",
                name_str
            );
            let doc = &format!(
                "This module holds the static implementation for `{}::now()`",
                name_str
            );
            let user_imports = &app.user_imports;

            let default_monotonic = if monotonic.args.default {
                quote!(pub use #name::now;)
            } else {
                quote!()
            };

            quote! {
                #default_monotonic

                #[doc = #doc]
                #[allow(non_snake_case)]
                pub mod #name {
                    #(
                        #[allow(unused_imports)]
                        #user_imports
                    )*

                    /// Read the current time from this monotonic
                    pub fn now() -> rtic::time::Instant<#ty> {
                        rtic::export::interrupt::free(|_| {
                            use rtic::Monotonic as _;
                            use rtic::time::Clock as _;
                            if let Some(m) = unsafe{ #app_path::#ident.get_mut_unchecked() } {
                                if let Ok(v) = m.try_now() {
                                    v
                                } else {
                                    unreachable!("Your monotonic is not infallible!")
                                }
                            } else {
                                panic!(#panic_str);
                            }
                        })
                    }
                }
            }
        })
        .collect();

    let monotonics = if !monotonic_parts.is_empty() {
        quote!(
            pub use rtic::Monotonic as _;

            /// Holds static methods for each monotonic.
            pub mod monotonics {
                #(
                    #[allow(unused_imports)]
                    #user_imports
                )*

                #(#monotonic_parts)*
            }
        )
    } else {
        quote!()
    };
    let rt_err = util::rt_err_ident();

    quote!(
        /// The RTIC application module
        pub mod #name {
            /// Always include the device crate which contains the vector table
            use #device as #rt_err;

            #monotonics

            #(#user_imports)*

            /// User code from within the module
            #(#user_code)*
            /// User code end

            #(#user)*

            #(#user_hardware_tasks)*

            #(#user_software_tasks)*

            #(#root)*

            #mod_resources

            #(#root_hardware_tasks)*

            #(#root_software_tasks)*

            /// app module
            #(#mod_app)*

            #(#mod_app_resources)*

            #(#mod_app_hardware_tasks)*

            #(#mod_app_software_tasks)*

            #(#mod_app_dispatchers)*

            #(#mod_app_timer_queue)*

            #(#mains)*
        }
    )
}

#[cfg(any(feature = "klee-analysis", feature = "klee-replay"))]
pub fn app(app: &App, analysis: &Analysis, extra: &Extra) -> TokenStream2 {
    let mut mod_app = vec![];
    let mut mains = vec![];
    let mut root = vec![];
    let mut user = vec![];

    // Generate the `main` function
    let assertion_stmts = assertions::codegen(app, analysis);

    let pre_init_stmts = pre_init::codegen(app, analysis, extra);

    let (mod_app_init, root_init, user_init, _call_init) = init::codegen(app, analysis, extra);

    let _post_init_stmts = post_init::codegen(app, analysis);

    let (mod_app_idle, root_idle, user_idle, _call_idle) = idle::codegen(app, analysis, extra);

    user.push(quote!(
        #user_init

        #user_idle
    ));

    root.push(quote!(
        #(#root_init)*

        #(#root_idle)*
    ));

    mod_app.push(quote!(
        #mod_app_init

        #mod_app_idle
    ));

    let main = util::suffixed("main");
    
    #[cfg(feature = "klee-analysis")]   
    {
        let klee_tasks = klee::codegen(app);
        
        mains.push(quote!(
            /// KLEE test harness
            mod rtic_ext {
                use super::*;
                use core::any::{Any,TypeId};
                use klee_rs::klee_make_symbolic;             

                fn late_type_supported<T: ?Sized + Any>(ty: &T, supported: &[TypeId]) -> bool {
                    let type_id = TypeId::of::<T>();

                    for supported_type in supported.iter() {
                        if &type_id == supported_type {
                            return true
                        }
                    }
                    false
                }

                #[no_mangle]
                unsafe extern "C" fn #main() {
                    let supported_late_types = [
                        TypeId::of::<core::mem::MaybeUninit<u8>>(),
                        TypeId::of::<core::mem::MaybeUninit<u16>>(),
                        TypeId::of::<core::mem::MaybeUninit<u32>>(),
                        TypeId::of::<core::mem::MaybeUninit<i8>>(),
                        TypeId::of::<core::mem::MaybeUninit<i16>>(),
                        TypeId::of::<core::mem::MaybeUninit<i32>>(),
                        TypeId::of::<core::mem::MaybeUninit<char>>(),
                    ];
                    #(#assertion_stmts)*
                    #(#klee_tasks)*
                }
            } 
        ));
    }

    #[cfg(feature = "klee-replay")]
    {
        let replay_tasks = klee_replay::codegen(app, analysis);

        mains.push(quote!(
            /// KLEE replay harness
            mod rtic_ext {
                use cortex_m::asm;
                use super::*;
                #[no_mangle]
                unsafe extern "C" fn #main() -> ! {
                    #(#assertion_stmts)*

                    #(#pre_init_stmts)*

                    // Enable trace
                    core.DCB.enable_trace();
                    core.DWT.enable_cycle_counter();

                    loop {
                        // Reset CYCCNT after each loop 
                        core.DWT.cyccnt.write(0);
                        /// 255: Replay start
                        asm::bkpt_imm(255);
                        #(#replay_tasks)*
                    }
                }
            }
        ))
    }

    let (mod_app_resources, mod_resources) = resources::codegen(app, analysis, extra);

    let (mod_app_hardware_tasks, root_hardware_tasks, user_hardware_tasks) =
        hardware_tasks::codegen(app, analysis, extra);

    let (mod_app_software_tasks, root_software_tasks, user_software_tasks) =
        software_tasks::codegen(app, analysis, extra);

    let mod_app_dispatchers = dispatchers::codegen(app, analysis, extra);
    let mod_app_timer_queue = timer_queue::codegen(app, analysis, extra);
    let user_imports = &app.user_imports;
    let user_code = &app.user_code;
    let name = &app.name;
    let device = &extra.device;
    let app_name = &app.name;
    let app_path = quote! {crate::#app_name};

    let monotonic_parts: Vec<_> = app
        .monotonics
        .iter()
        .map(|(_, monotonic)| {
            let name = &monotonic.ident;
            let name_str = &name.to_string();
            let ty = &monotonic.ty;
            let ident = util::monotonic_ident(&name_str);
            let ident = util::mark_internal_ident(&ident);
            let panic_str = &format!(
                "Use of monotonic '{}' before it was passed to the runtime",
                name_str
            );
            let doc = &format!(
                "This module holds the static implementation for `{}::now()`",
                name_str
            );
            let user_imports = &app.user_imports;

            let default_monotonic = if monotonic.args.default {
                quote!(pub use #name::now;)
            } else {
                quote!()
            };

            quote! {
                #default_monotonic

                #[doc = #doc]
                #[allow(non_snake_case)]
                pub mod #name {
                    #(
                        #[allow(unused_imports)]
                        #user_imports
                    )*

                    /// Read the current time from this monotonic
                    pub fn now() -> rtic::time::Instant<#ty> {
                        rtic::export::interrupt::free(|_| {
                            use rtic::Monotonic as _;
                            use rtic::time::Clock as _;
                            if let Some(m) = unsafe{ #app_path::#ident.as_ref() } {
                                if let Ok(v) = m.try_now() {
                                    v
                                } else {
                                    unreachable!("Your monotonic is not infallible!")
                                }
                            } else {
                                panic!(#panic_str);
                            }
                        })
                    }
                }
            }
        })
        .collect();
    let monotonics = if !monotonic_parts.is_empty() {
        quote!(
            pub use rtic::Monotonic as _;

            /// Holds static methods for each monotonic.
            pub mod monotonics {
                #(
                    #[allow(unused_imports)]
                    #user_imports
                )*

                #(#monotonic_parts)*
            }
        )
    } else {
        quote!()
    };
    let rt_err = util::rt_err_ident();

    quote!(
        /// The RTIC application module
        pub mod #name {
            /// Always include the device crate which contains the vector table
            use #device as #rt_err;

            #monotonics

            #(#user_imports)*

            /// User code from within the module
            #(#user_code)*
            /// User code end

            #(#user)*

            #(#user_hardware_tasks)*

            #(#user_software_tasks)*

            #(#root)*

            #mod_resources

            #(#root_hardware_tasks)*

            #(#root_software_tasks)*

            /// app module
            #(#mod_app)*

            #(#mod_app_resources)*

            #(#mod_app_hardware_tasks)*

            #(#mod_app_software_tasks)*

            #(#mod_app_dispatchers)*

            #(#mod_app_timer_queue)*

            /// Set as a global variable in order to not optimize out the replay harness
            static mut __klee_task_id: u8 = 0;
            #(#mains)*
        }
    )
}
