use crate::{analyze::Analysis, check::Extra, codegen::util};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::{ast::App, Context};

#[cfg(not(any(feature = "klee-analysis", feature = "klee-replay")))]
pub fn codegen(
    ctxt: Context,
    resources_tick: bool,
    app: &App,
    analysis: &Analysis,
    extra: &Extra,
) -> TokenStream2 {
    let mut items = vec![];
    let mut module_items = vec![];
    let mut fields = vec![];
    let mut values = vec![];
    // Used to copy task cfgs to the whole module
    let mut task_cfgs = vec![];

    let name = ctxt.ident(app);

    let mut lt = None;
    match ctxt {
        Context::Init => {
            fields.push(quote!(
                /// Core (Cortex-M) peripherals
                pub core: rtic::export::Peripherals
            ));

            if extra.peripherals {
                let device = &extra.device;

                fields.push(quote!(
                    /// Device peripherals
                    pub device: #device::Peripherals
                ));

                values.push(quote!(device: #device::Peripherals::steal()));
            }

            lt = Some(quote!('a));
            fields.push(quote!(
                /// Critical section token for init
                pub cs: rtic::export::CriticalSection<#lt>
            ));

            values.push(quote!(cs: rtic::export::CriticalSection::new()));

            values.push(quote!(core));
        }

        Context::Idle => {}

        Context::HardwareTask(_) => {}

        Context::SoftwareTask(_) => {}
    }

    if ctxt.has_locals(app) {
        let ident = util::locals_ident(ctxt, app);
        module_items.push(quote!(
            #[doc(inline)]
            pub use super::#ident as Locals;
        ));
    }

    if ctxt.has_resources(app) {
        let ident = util::resources_ident(ctxt, app);
        let ident = util::mark_internal_ident(&ident);
        let lt = if resources_tick {
            lt = Some(quote!('a));
            Some(quote!('a))
        } else {
            None
        };

        module_items.push(quote!(
            #[doc(inline)]
            pub use super::#ident as Resources;
        ));

        fields.push(quote!(
            /// Resources this task has access to
            pub resources: #name::Resources<#lt>
        ));

        let priority = if ctxt.is_init() {
            None
        } else {
            Some(quote!(priority))
        };
        values.push(quote!(resources: #name::Resources::new(#priority)));
    }

    if let Context::Init = ctxt {
        let late_fields = analysis
            .late_resources
            .iter()
            .flat_map(|resources| {
                resources.iter().map(|name| {
                    let ty = &app.late_resources[name].ty;
                    let cfgs = &app.late_resources[name].cfgs;

                    quote!(
                        #(#cfgs)*
                        pub #name: #ty
                    )
                })
            })
            .collect::<Vec<_>>();

        let internal_late_ident = util::mark_internal_name("LateResources");
        items.push(quote!(
            /// Resources initialized at runtime
            #[allow(non_snake_case)]
            pub struct #internal_late_ident {
                #(#late_fields),*
            }
        ));
        module_items.push(quote!(
            pub use super::#internal_late_ident as LateResources;
        ));

        let monotonic_types: Vec<_> = app
            .monotonics
            .iter()
            .map(|(_, monotonic)| {
                let mono = &monotonic.ty;
                quote! {#mono}
            })
            .collect();

        let internal_monotonics_ident = util::mark_internal_name("Monotonics");

        items.push(quote!(
            /// Monotonics used by the system
            #[allow(non_snake_case)]
            pub struct #internal_monotonics_ident(
                #(pub #monotonic_types),*
            );
        ));

        module_items.push(quote!(
            pub use super::#internal_monotonics_ident as Monotonics;
        ));
    }

    let doc = match ctxt {
        Context::Idle => "Idle loop",
        Context::Init => "Initialization function",
        Context::HardwareTask(_) => "Hardware task",
        Context::SoftwareTask(_) => "Software task",
    };

    let v = Vec::new();
    let cfgs = match ctxt {
        Context::HardwareTask(t) => {
            &app.hardware_tasks[t].cfgs
            // ...
        }
        Context::SoftwareTask(t) => {
            &app.software_tasks[t].cfgs
            // ...
        }
        _ => &v,
    };

    let core = if ctxt.is_init() {
        Some(quote!(core: rtic::export::Peripherals,))
    } else {
        None
    };

    let priority = if ctxt.is_init() {
        None
    } else {
        Some(quote!(priority: &#lt rtic::export::Priority))
    };

    let internal_context_name = util::internal_task_ident(name, "Context");

    items.push(quote!(
        #(#cfgs)*
        /// Execution context
        pub struct #internal_context_name<#lt> {
            #(#fields,)*
        }

        #(#cfgs)*
        impl<#lt> #internal_context_name<#lt> {
            #[inline(always)]
            pub unsafe fn new(#core #priority) -> Self {
                #internal_context_name {
                    #(#values,)*
                }
            }
        }
    ));

    module_items.push(quote!(
        #(#cfgs)*
        pub use super::#internal_context_name as Context;
    ));

    // not sure if this is the right way, maybe its backwards,
    // that spawn_module should put in in root

    if let Context::SoftwareTask(..) = ctxt {
        let spawnee = &app.software_tasks[name];
        let priority = spawnee.args.priority;
        let t = util::spawn_t_ident(priority);
        let cfgs = &spawnee.cfgs;
        // Store a copy of the task cfgs
        task_cfgs = cfgs.clone();
        let (args, tupled, untupled, ty) = util::regroup_inputs(&spawnee.inputs);
        let args = &args;
        let tupled = &tupled;
        let fq = util::fq_ident(name);
        let fq = util::mark_internal_ident(&fq);
        let rq = util::rq_ident(priority);
        let rq = util::mark_internal_ident(&rq);
        let inputs = util::inputs_ident(name);
        let inputs = util::mark_internal_ident(&inputs);

        let device = &extra.device;
        let enum_ = util::interrupt_ident();
        let interrupt = &analysis
            .interrupts
            .get(&priority)
            .expect("RTIC-ICE: interrupt identifer not found")
            .0;

        let internal_spawn_ident = util::internal_task_ident(name, "spawn");

        // Spawn caller
        items.push(quote!(

        #(#cfgs)*
        /// Spawns the task directly
        pub fn #internal_spawn_ident(#(#args,)*) -> Result<(), #ty> {
            let input = #tupled;

            unsafe {
                if let Some(index) = rtic::export::interrupt::free(|_| #fq.get_mut_unchecked().dequeue()) {
                    #inputs
                        .get_mut_unchecked()
                        .get_unchecked_mut(usize::from(index))
                        .as_mut_ptr()
                        .write(input);

                    rtic::export::interrupt::free(|_| {
                        #rq.get_mut_unchecked().enqueue_unchecked((#t::#name, index));
                    });

                    rtic::pend(#device::#enum_::#interrupt);

                    Ok(())
                } else {
                    Err(input)
                }
            }

        }));

        module_items.push(quote!(
            #(#cfgs)*
            pub use super::#internal_spawn_ident as spawn;
        ));

        // Schedule caller
        for (_, monotonic) in &app.monotonics {
            let instants = util::monotonic_instants_ident(name, &monotonic.ident);
            let instants = util::mark_internal_ident(&instants);
            let monotonic_name = monotonic.ident.to_string();

            let tq = util::tq_ident(&monotonic.ident.to_string());
            let tq = util::mark_internal_ident(&tq);
            let t = util::schedule_t_ident();
            let m = &monotonic.ident;
            let mono_type = &monotonic.ident;
            let m_ident = util::monotonic_ident(&monotonic_name);
            let m_ident = util::mark_internal_ident(&m_ident);
            let m_isr = &monotonic.args.binds;
            let enum_ = util::interrupt_ident();

            let (enable_interrupt, pend) = if &*m_isr.to_string() == "SysTick" {
                (
                    quote!(core::mem::transmute::<_, cortex_m::peripheral::SYST>(())
                        .enable_interrupt()),
                    quote!(cortex_m::peripheral::SCB::set_pendst()),
                )
            } else {
                let rt_err = util::rt_err_ident();
                (
                    quote!(rtic::export::NVIC::unmask(#rt_err::#enum_::#m_isr)),
                    quote!(rtic::pend(#rt_err::#enum_::#m_isr)),
                )
            };

            let tq_marker = util::mark_internal_ident(&util::timer_queue_marker_ident());

            // For future use
            // let doc = format!(" RTIC internal: {}:{}", file!(), line!());
            // items.push(quote!(#[doc = #doc]));
            let internal_spawn_handle_ident =
                util::internal_monotonics_ident(name, m, "SpawnHandle");
            let internal_spawn_at_ident = util::internal_monotonics_ident(name, m, "spawn_at");
            let internal_spawn_after_ident =
                util::internal_monotonics_ident(name, m, "spawn_after");

            if monotonic.args.default {
                module_items.push(quote!(
                    pub use #m::spawn_after;
                    pub use #m::spawn_at;
                    pub use #m::SpawnHandle;
                ));
            }
            module_items.push(quote!(
                pub mod #m {
                    pub use super::super::#internal_spawn_after_ident as spawn_after;
                    pub use super::super::#internal_spawn_at_ident as spawn_at;
                    pub use super::super::#internal_spawn_handle_ident as SpawnHandle;
                }
            ));

            items.push(quote!(
                pub struct #internal_spawn_handle_ident {
                    #[doc(hidden)]
                    marker: u32,
                }

                impl #internal_spawn_handle_ident {
                    pub fn cancel(self) -> Result<#ty, ()> {
                        rtic::export::interrupt::free(|_| unsafe {
                            let tq = &mut *#tq.get_mut_unchecked().as_mut_ptr();
                            if let Some((_task, index)) = tq.cancel_marker(self.marker) {
                                // Get the message
                                let msg = #inputs
                                    .get_unchecked()
                                    .get_unchecked(usize::from(index))
                                    .as_ptr()
                                    .read();
                                // Return the index to the free queue
                                #fq.get_mut_unchecked().split().0.enqueue_unchecked(index);

                                Ok(msg)
                            } else {
                                Err(())
                            }
                        })
                    }

                    #[inline]
                    pub fn reschedule_after<D>(self, duration: D) -> Result<Self, ()>
                        where D: rtic::time::duration::Duration + rtic::time::fixed_point::FixedPoint,
                                 D::T: Into<<#mono_type as rtic::time::Clock>::T>,
                    {
                        self.reschedule_at(monotonics::#m::now() + duration)
                    }

                    pub fn reschedule_at(self, instant: rtic::time::Instant<#mono_type>) -> Result<Self, ()>
                    {
                        rtic::export::interrupt::free(|_| unsafe {
                            let marker = *#tq_marker.get_mut_unchecked();
                            *#tq_marker.get_mut_unchecked() = #tq_marker.get_mut_unchecked().wrapping_add(1);

                            let tq = &mut *#tq.get_mut_unchecked().as_mut_ptr();

                            tq.update_marker(self.marker, marker, instant, || #pend).map(|_| #name::#m::SpawnHandle { marker })
                        })
                    }
                }

                #(#cfgs)*
                /// Spawns the task after a set duration relative to the current time
                ///
                /// This will use the time `Instant::new(0)` as baseline if called in `#[init]`,
                /// so if you use a non-resetable timer use `spawn_at` when in `#[init]`
                pub fn #internal_spawn_after_ident<D>(
                    duration: D
                    #(,#args)*
                ) -> Result<#name::#m::SpawnHandle, #ty>
                    where D: rtic::time::duration::Duration + rtic::time::fixed_point::FixedPoint,
                        D::T: Into<<#mono_type as rtic::time::Clock>::T>,
                {

                    let instant = if rtic::export::interrupt::free(|_| unsafe { #m_ident.get_mut_unchecked().is_none() }) {
                        rtic::time::Instant::new(0)
                    } else {
                        monotonics::#m::now()
                    };

                    #internal_spawn_at_ident(instant + duration #(,#untupled)*)
                }

                #(#cfgs)*
                /// Spawns the task at a fixed time instant
                pub fn #internal_spawn_at_ident(
                    instant: rtic::time::Instant<#mono_type>
                    #(,#args)*
                ) -> Result<#name::#m::SpawnHandle, #ty> {
                    unsafe {
                        let input = #tupled;
                        if let Some(index) = rtic::export::interrupt::free(|_| #fq.get_mut_unchecked().dequeue()) {
                            #inputs
                                .get_mut_unchecked()
                                .get_unchecked_mut(usize::from(index))
                                .as_mut_ptr()
                                .write(input);

                            #instants
                                .get_mut_unchecked()
                                .get_unchecked_mut(usize::from(index))
                                .as_mut_ptr()
                                .write(instant);

                            rtic::export::interrupt::free(|_| {
                                let marker = *#tq_marker.get_mut_unchecked();
                                let nr = rtic::export::NotReady {
                                    instant,
                                    index,
                                    task: #t::#name,
                                    marker,
                                };

                                *#tq_marker.get_mut_unchecked() = #tq_marker.get_mut_unchecked().wrapping_add(1);

                                let tq = &mut *#tq.get_mut_unchecked().as_mut_ptr();

                                tq.enqueue_unchecked(
                                    nr,
                                    || #enable_interrupt,
                                    || #pend,
                                    #m_ident.get_mut_unchecked().as_mut());

                                Ok(#name::#m::SpawnHandle { marker })
                            })
                        } else {
                            Err(input)
                        }
                    }
                }
            ));
        }
    }

    if !items.is_empty() {
        quote!(
            #(#items)*

            #[allow(non_snake_case)]
            #(#task_cfgs)*
            #[doc = #doc]
            pub mod #name {
                #(#module_items)*
            }
        )
    } else {
        quote!()
    }
}

#[cfg(any(feature = "klee-analysis", feature = "klee-replay"))]
pub fn codegen(
    ctxt: Context,
    resources_tick: bool,
    app: &App,
    analysis: &Analysis,
    extra: &Extra,
) -> TokenStream2 {
    let mut items = vec![];
    let mut module_items = vec![];
    let mut fields = vec![];
    let mut values = vec![];
    // Used to copy task cfgs to the whole module
    let mut task_cfgs = vec![];

    let name = ctxt.ident(app);

    let mut lt = None;
    match ctxt {
        Context::Init => {
            fields.push(quote!(
                /// Core (Cortex-M) peripherals
                pub core: rtic::export::Peripherals
            ));

            if extra.peripherals {
                let device = &extra.device;

                fields.push(quote!(
                    /// Device peripherals
                    pub device: #device::Peripherals
                ));

                values.push(quote!(device: #device::Peripherals::steal()));
            }

            lt = Some(quote!('a));
            fields.push(quote!(
                /// Critical section token for init
                pub cs: rtic::export::CriticalSection<#lt>
            ));

            values.push(quote!(cs: rtic::export::CriticalSection::new()));

            values.push(quote!(core));
        }

        Context::Idle => {}

        Context::HardwareTask(_) => {}

        Context::SoftwareTask(_) => {}
    }

    if ctxt.has_locals(app) {
        let ident = util::locals_ident(ctxt, app);
        module_items.push(quote!(
            #[doc(inline)]
            pub use super::#ident as Locals;
        ));
    }

    if ctxt.has_resources(app) {
        let ident = util::resources_ident(ctxt, app);
        let ident = util::mark_internal_ident(&ident);
        let lt = if resources_tick {
            lt = Some(quote!('a));
            Some(quote!('a))
        } else {
            None
        };

        module_items.push(quote!(
            #[doc(inline)]
            pub use super::#ident as Resources;
        ));

        fields.push(quote!(
            /// Resources this task has access to
            pub resources: #name::Resources<#lt>
        ));

        let priority = if ctxt.is_init() {
            None
        } else {
            Some(quote!(priority))
        };
        values.push(quote!(resources: #name::Resources::new(#priority)));
    }

    if let Context::Init = ctxt {
        let late_fields = analysis
            .late_resources
            .iter()
            .flat_map(|resources| {
                resources.iter().map(|name| {
                    let ty = &app.late_resources[name].ty;
                    let cfgs = &app.late_resources[name].cfgs;

                    quote!(
                        #(#cfgs)*
                        pub #name: #ty
                    )
                })
            })
            .collect::<Vec<_>>();

        let internal_late_ident = util::mark_internal_name("LateResources");
        items.push(quote!(
            /// Resources initialized at runtime
            #[allow(non_snake_case)]
            pub struct #internal_late_ident {
                #(#late_fields),*
            }
        ));
        module_items.push(quote!(
            pub use super::#internal_late_ident as LateResources;
        ));

        let monotonic_types: Vec<_> = app
            .monotonics
            .iter()
            .map(|(_, monotonic)| {
                let mono = &monotonic.ty;
                quote! {#mono}
            })
            .collect();
        
        let internal_monotonics_ident = util::mark_internal_name("Monotonics");

        items.push(quote!(
            /// Monotonics used by the system
            #[allow(non_snake_case)]
            pub struct #internal_monotonics_ident(
                #(pub #monotonic_types),*
            );
        ));

        module_items.push(quote!(
            pub use super::#internal_monotonics_ident as Monotonics;
        ));
    }

    let doc = match ctxt {
        Context::Idle => "Idle loop",
        Context::Init => "Initialization function",
        Context::HardwareTask(_) => "Hardware task",
        Context::SoftwareTask(_) => "Software task",
    };

    let v = Vec::new();
    let cfgs = match ctxt {
        Context::HardwareTask(t) => {
            &app.hardware_tasks[t].cfgs
            // ...
        }
        Context::SoftwareTask(t) => {
            &app.software_tasks[t].cfgs
            // ...
        }
        _ => &v,
    };

    let core = if ctxt.is_init() {
        Some(quote!(core: rtic::export::Peripherals,))
    } else {
        None
    };

    let priority = if ctxt.is_init() {
        None
    } else {
        Some(quote!(priority: &#lt rtic::export::Priority))
    };

    let internal_context_name = util::internal_task_ident(name, "Context");

    items.push(quote!(
        #(#cfgs)*
        /// Execution context
        pub struct #internal_context_name<#lt> {
            #(#fields,)*
        }

        #(#cfgs)*
        impl<#lt> #internal_context_name<#lt> {
            #[inline(always)]
            pub unsafe fn new(#core #priority) -> Self {
                #internal_context_name {
                    #(#values,)*
                }
            }
        }
    ));

    module_items.push(quote!(
        #(#cfgs)*
        pub use super::#internal_context_name as Context;
    ));

    // not sure if this is the right way, maybe its backwards,
    // that spawn_module should put in in root

    if let Context::SoftwareTask(..) = ctxt {
        let spawnee = &app.software_tasks[name];
        let priority = spawnee.args.priority;
        let _t = util::spawn_t_ident(priority);
        let cfgs = &spawnee.cfgs;
        // Store a copy of the task cfgs
        task_cfgs = cfgs.clone();
        let (args, tupled, untupled, ty) = util::regroup_inputs(&spawnee.inputs);
        let args = &args;
        let tupled = &tupled;
        let fq = util::fq_ident(name);
        let fq = util::mark_internal_ident(&fq);
        let rq = util::rq_ident(priority);
        let rq = util::mark_internal_ident(&rq);
        let inputs = util::inputs_ident(name);
        let inputs = util::mark_internal_ident(&inputs);

        let device = &extra.device;
        let enum_ = util::interrupt_ident();
        let interrupt = &analysis
            .interrupts
            .get(&priority)
            .expect("RTIC-ICE: interrupt identifer not found")
            .0;
        
        let internal_spawn_ident = util::internal_task_ident(name, "spawn");

        // Spawn caller
        items.push(quote!(

        #(#cfgs)*
        /// Spawns the task directly
        pub fn #internal_spawn_ident(#(#args,)*) -> Result<(), #ty> {
            Ok(())
        }));

        module_items.push(quote!(
            #(#cfgs)*
            pub use super::#internal_spawn_ident as spawn;
        ));

        // Schedule caller
        for (_, monotonic) in &app.monotonics {
            let instants = util::monotonic_instants_ident(name, &monotonic.ident);
            let instants = util::mark_internal_ident(&instants);
            let monotonic_name = monotonic.ident.to_string();

            let tq = util::tq_ident(&monotonic.ident.to_string());
            let tq = util::mark_internal_ident(&tq);
            let t = util::schedule_t_ident();
            let m = &monotonic.ident;
            let mono_type = &monotonic.ident;
            let m_ident = util::monotonic_ident(&monotonic_name);
            let m_ident = util::mark_internal_ident(&m_ident);
            let m_isr = &monotonic.args.binds;
            let enum_ = util::interrupt_ident();

            let (enable_interrupt, pend) = if &*m_isr.to_string() == "SysTick" {
                (
                    quote!(core::mem::transmute::<_, cortex_m::peripheral::SYST>(())
                        .enable_interrupt()),
                    quote!(cortex_m::peripheral::SCB::set_pendst()),
                )
            } else {
                let rt_err = util::rt_err_ident();
                (
                    quote!(rtic::export::NVIC::unmask(#rt_err::#enum_::#m_isr)),
                    quote!(rtic::pend(#rt_err::#enum_::#m_isr)),
                )
            };

            let tq_marker = util::mark_internal_ident(&util::timer_queue_marker_ident());

            // For future use
            // let doc = format!(" RTIC internal: {}:{}", file!(), line!());
            // items.push(quote!(#[doc = #doc]));
            let internal_spawn_handle_ident =
                util::internal_monotonics_ident(name, m, "SpawnHandle");
            let internal_spawn_at_ident = util::internal_monotonics_ident(name, m, "spawn_at");
            let internal_spawn_after_ident =
                util::internal_monotonics_ident(name, m, "spawn_after");

            if monotonic.args.default {
                module_items.push(quote!(
                    pub use #m::spawn_after;
                    pub use #m::spawn_at;
                    pub use #m::SpawnHandle;
                ));
            }
            module_items.push(quote!(
                pub mod #m {
                    pub use super::super::#internal_spawn_after_ident as spawn_after;
                    pub use super::super::#internal_spawn_at_ident as spawn_at;
                    pub use super::super::#internal_spawn_handle_ident as SpawnHandle;
                }
            ));

            items.push(quote!(
                pub struct #internal_spawn_handle_ident {
                    #[doc(hidden)]
                    marker: u32,
                }

                impl #internal_spawn_handle_ident {
                    pub fn cancel(self) -> Result<#ty, ()> {
                        rtic::export::interrupt::free(|_| unsafe {
                            let tq = &mut *#tq.get_mut_unchecked().as_mut_ptr();
                            if let Some((_task, index)) = tq.cancel_marker(self.marker) {
                                // Get the message
                                let msg = #inputs
                                    .get_unchecked()
                                    .get_unchecked(usize::from(index))
                                    .as_ptr()
                                    .read();
                                // Return the index to the free queue
                                #fq.get_mut_unchecked().split().0.enqueue_unchecked(index);

                                Ok(msg)
                            } else {
                                Err(())
                            }
                        })
                    }

                    #[inline]
                    pub fn reschedule_after<D>(self, duration: D) -> Result<Self, ()>
                        where D: rtic::time::duration::Duration + rtic::time::fixed_point::FixedPoint,
                                 D::T: Into<<#mono_type as rtic::time::Clock>::T>,
                    {
                        self.reschedule_at(monotonics::#m::now() + duration)
                    }

                    pub fn reschedule_at(self, instant: rtic::time::Instant<#mono_type>) -> Result<Self, ()>
                    {
                        rtic::export::interrupt::free(|_| unsafe {
                            let marker = *#tq_marker.get_mut_unchecked();
                            *#tq_marker.get_mut_unchecked() = #tq_marker.get_mut_unchecked().wrapping_add(1);

                            let tq = &mut *#tq.get_mut_unchecked().as_mut_ptr();

                            tq.update_marker(self.marker, marker, instant, || #pend).map(|_| #name::#m::SpawnHandle { marker })
                        })
                    }
                }

                #(#cfgs)*
                /// Spawns the task after a set duration relative to the current time
                ///
                /// This will use the time `Instant::new(0)` as baseline if called in `#[init]`,
                /// so if you use a non-resetable timer use `spawn_at` when in `#[init]`
                pub fn #internal_spawn_after_ident<D>(
                    duration: D
                    #(,#args)*
                ) -> Result<#name::#m::SpawnHandle, #ty>
                    where D: rtic::time::duration::Duration + rtic::time::fixed_point::FixedPoint,
                        D::T: Into<<#mono_type as rtic::time::Clock>::T>,
                {

                    let instant = if rtic::export::interrupt::free(|_| unsafe { #m_ident.get_mut_unchecked().is_none() }) {
                        rtic::time::Instant::new(0)
                    } else {
                        monotonics::#m::now()
                    };

                    #internal_spawn_at_ident(instant + duration #(,#untupled)*)
                }

                #(#cfgs)*
                /// Spawns the task at a fixed time instant
                pub fn #internal_spawn_at_ident(
                    instant: rtic::time::Instant<#mono_type>
                    #(,#args)*
                ) -> Result<#name::#m::SpawnHandle, #ty> {
                    unsafe {
                        let input = #tupled;
                        if let Some(index) = rtic::export::interrupt::free(|_| #fq.get_mut_unchecked().dequeue()) {
                            #inputs
                                .get_mut_unchecked()
                                .get_unchecked_mut(usize::from(index))
                                .as_mut_ptr()
                                .write(input);

                            #instants
                                .get_mut_unchecked()
                                .get_unchecked_mut(usize::from(index))
                                .as_mut_ptr()
                                .write(instant);

                            rtic::export::interrupt::free(|_| {
                                let marker = *#tq_marker.get_mut_unchecked();
                                let nr = rtic::export::NotReady {
                                    instant,
                                    index,
                                    task: #t::#name,
                                    marker,
                                };

                                *#tq_marker.get_mut_unchecked() = #tq_marker.get_mut_unchecked().wrapping_add(1);

                                let tq = &mut *#tq.get_mut_unchecked().as_mut_ptr();

                                tq.enqueue_unchecked(
                                    nr,
                                    || #enable_interrupt,
                                    || #pend,
                                    #m_ident.get_mut_unchecked().as_mut());

                                Ok(#name::#m::SpawnHandle { marker })
                            })
                        } else {
                            Err(input)
                        }
                    }
                }
            ));
        }
    }

    if !items.is_empty() {
        quote!(
            #(#items)*

            #[allow(non_snake_case)]
            #(#task_cfgs)*
            #[doc = #doc]
            pub mod #name {
                #(#module_items)*
            }
        )
    } else {
        quote!()
    }
}
