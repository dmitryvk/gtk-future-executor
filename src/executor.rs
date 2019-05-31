use futures::prelude::*;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::cell::RefCell;
use std::collections::HashMap;

type BoxUnitFuture = Box<Future<Item=(), Error=()>>;

struct GtkEventLoopAsyncExecutorBackend {
    next_id: AtomicUsize,
    spawns: RefCell<HashMap<usize, futures::executor::Spawn<BoxUnitFuture>>>,
}

/// An executor that executes futures on Gtk+ main loop.
/// This allows executing asynchronous code that manipulates Gtk+ widgets.
/// 
/// Usage:
/// 1) Create with GtkEventLoopAsyncExecutor::new()
/// 2) Clone as necessary (all clones refer to the same executor)
/// 3) Use GtkEventLoopAsyncExecutor::spawn() to launch new async GUI code
/// 
/// GtkEventLoopAsyncExecutor ensures memory- and thread-safety by being not shareable or sendable between threads.
/// This is a requirement for GUI code.
/// 
/// Example: 
/// ```rust
/// use futures::prelude::*;
/// use futures::future;
/// use futures_cpupool::CpuPool;
/// 
/// use gtk_future_executor::GtkEventLoopAsyncExecutor;
/// use gtk_future_executor::Promise;
/// use gtk::prelude::*;
/// 
/// // An examples that computes Fibonacci numbers in background
/// 
/// fn main() -> Result<(), String> {
/// 
///     gtk::init().map_err(|_| "Failed to initialize Gtk+".to_string())?;
/// 
///     // Constuct new executor
///     let gtk_executor = GtkEventLoopAsyncExecutor::new();
///     // This examples uses CPU pool for invoking long-running computation in background
///     let cpu_pool = CpuPool::new_num_cpus();
/// 
///     let fut_main = gui_main(cpu_pool.clone(), gtk_executor.clone())
///         .then(|_| {
///             // Exit main loop when gui_main() finishes
///             gtk::main_quit();
/// 
///             future::ok(())
///         });
/// 
///     // This executes the async main function inside Gtk+ event loop
///     gtk_executor.spawn(fut_main);
/// 
///     gtk::main();
/// 
///     Result::Ok(())
/// }
/// 
/// // An async function that shows a window. Returned future will resolve when user closes the window.
/// fn gui_main(cpu_pool: CpuPool, gtk_executor: GtkEventLoopAsyncExecutor) -> impl Future<Item=(), Error=String> {
/// 
///     let promise = Promise::new();
/// 
///     let window = gtk::Window::new(gtk::WindowType::Toplevel);
///     let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
///     let label = gtk::Label::new("Enter n:");
///     let result_label = gtk::Label::new("<result>");
///     let textbox = gtk::Entry::new();
///     let button = gtk::Button::new_with_label("OK");
/// 
///     window.add(&vbox);
///     vbox.pack_start(&label, false, true, 0);
///     vbox.pack_start(&textbox, false, true, 0);
///     vbox.pack_start(&button, false, true, 0);
///     vbox.pack_start(&result_label, false, true, 0);
/// 
///     window.set_title("Fib");
///     window.set_position(gtk::WindowPosition::Center);
/// 
///     {
///         let promise = promise.clone();
///         window.connect_delete_event(move |_, _| {
///             promise.resolve(());
/// 
///             Inhibit(false)
///         });
///     }
/// 
///     {
///         let cpu_pool = cpu_pool.clone();
///         let gtk_executor = gtk_executor.clone();
///         let textbox = textbox.clone();
///         let result_label = result_label.clone();
///         button.connect_clicked(move |_| {
/// 
///             let opt_text = textbox.get_text();
///             let text = opt_text.as_ref().map(|s| s.as_str()).unwrap_or("");
///             let n: u64 = match text.parse() {
///                 Ok(x) => x,
///                 Err(x) => {
///                     eprintln!("Error: {}", x);
///                     return;
///                 }
///             };
///             result_label.set_text("computing...");
///             let result_label = result_label.clone();
/// 
///             // With GtkEventLoopAsyncExecutor we can await the long running async computation
///             // and continue manipulating GUI widgets on the main thread.
///             gtk_executor.spawn(
///                 // cpu_pool execute `compute_fib` in background thread_pool
///                 cpu_pool.spawn_fn(move || future::ok(compute_fib(n)))
///                     .and_then(move |r| {
///                         // this code is executed on main thread
///                         result_label.set_text(&format!("fib({}) = {}", n, r));
/// 
///                         future::ok(())
///                     })
///             );
///         });
///     }
/// 
///     window.show_all();
/// 
///     promise
/// }
/// 
/// // Fibonacci function. This function will take very long time for large values of `n`.
/// fn compute_fib(n: u64) -> u64 {
///     if n < 2 {
///         1
///     } else {
///         compute_fib(n - 2) + compute_fib(n - 1)
///     }
/// }
/// ```
#[derive(Clone)]
pub struct GtkEventLoopAsyncExecutor {
    backend: Arc<GtkEventLoopAsyncExecutorBackend>,
}

#[derive(Clone)]
struct GtkEventLoopAsyncExecutorNotifier {
    executor: GtkEventLoopAsyncExecutor,
}

impl GtkEventLoopAsyncExecutorNotifier {
    pub fn new(executor: GtkEventLoopAsyncExecutor) -> Self {
        GtkEventLoopAsyncExecutorNotifier {
            executor
        }
    }
}

impl GtkEventLoopAsyncExecutor {
    /// Instantiates new executor. May only be called from Gtk+ main thread. Gtk+ must be initialized.
    /// *Panics* if called before Gtk+ initialization or from non-main thread.
    pub fn new() -> Self {
        assert!(gtk::is_initialized_main_thread(), "GtkEventLoopAsyncExecutor::new() may only be called on Gtk+ main thread");

        GtkEventLoopAsyncExecutor {
            backend: Arc::new(
                GtkEventLoopAsyncExecutorBackend {
                    next_id: AtomicUsize::new(0),
                    spawns: RefCell::new(HashMap::new())
                }
            )
        }
    }

    /// Executes specified future on Gtk+ main thread (using event loop to schedule callbacks)
    pub fn spawn<F: Future<Item=(), Error=()> + Sized + 'static>(&self, f: F) {
        let id = self.backend.next_id.fetch_add(1, Ordering::SeqCst);
        {
            let mut spawns = self.backend.spawns.borrow_mut();
            let spawn = futures::executor::spawn(Box::new(f) as BoxUnitFuture);
            spawns.insert(id, spawn);
        }

        let handle = GtkEventLoopAsyncExecutorNotifier::new(self.clone());

        use futures::executor::Notify;

        handle.notify(id);
    }

    fn invoke(&self, id: usize) {
        let opt_spawn = self.backend.spawns.borrow_mut().remove(&id);
        match opt_spawn {
            None => {
                eprintln!("Attempted to invoke non-existing spawn {}", id);
            },
            Some(mut spawn) => {
                let result = spawn.poll_future_notify(
                    &futures::executor::NotifyHandle::from(
                        Arc::new(GtkEventLoopAsyncExecutorNotifier::new(self.clone()))
                    ),
                    id
                );
                
                match result {
                    Ok(Async::Ready(_)) => {
                        // Do nothing
                    },
                    Ok(Async::NotReady) => {
                        self.backend.spawns.borrow_mut().insert(id, spawn);
                    },
                    Err(_) => {
                        eprintln!("Spawned future {} returned error", id);
                    }
                }
            }
        }
    }
}

// safety rationale:
// GtkEventLoopAsyncExecutorNotifier ensures that GtkEventLoopAsyncExecutor is only ever called from Gtk+ main loop.
// GtkEventLoopAsyncExecutor may only be created on Gtk+ main thread and main loop runs on main thread.
// Hence dereference of `executor` Arc happens only happens on the same thread that created GtkEventLoopAsyncExecutor.
unsafe impl Send for GtkEventLoopAsyncExecutorNotifier{}
unsafe impl Sync for GtkEventLoopAsyncExecutorNotifier{}

impl futures::executor::Notify for GtkEventLoopAsyncExecutorNotifier {
    fn notify(&self, id: usize) {
        let handle = self.clone();
        glib::source::idle_add(move || {
            handle.executor.invoke(id);
            glib::source::Continue(false)
        });
    }
}

