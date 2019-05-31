This crate provides basic building blocks for writing async GUI code with Gtk-rs:
1. `GtkEventLoopAsyncExecutor` - an executor for executing futures that may manipulate GUI widgets
2. `Promise` - an implementation of [futures::Future] that is often useful for GUI code


`Promise` is a future that can be completed or failed with `resolve` or `reject` methods.

`Promise` object is freely cloneable (all clones refer to the same underlying object) and is thread-safe.

`Promise` objects are handy for integrating `Future`-based code with non-`Future` based code.

`GtkEventLoopAsyncExecutor` is an executor that executes futures on Gtk+ main loop.
This allows executing asynchronous code that manipulates Gtk+ widgets.

Usage:
1) Create with GtkEventLoopAsyncExecutor::new()
2) Clone as necessary (all clones refer to the same executor)
3) Use GtkEventLoopAsyncExecutor::spawn() to launch new async GUI code

GtkEventLoopAsyncExecutor ensures memory- and thread-safety by being not shareable or sendable between threads.
This is a requirement for GUI code.

Example: 
```rust
use futures::prelude::*;
use futures::future;
use futures_cpupool::CpuPool;

use gtk_future_executor::GtkEventLoopAsyncExecutor;
use gtk_future_executor::Promise;
use gtk::prelude::*;

// An examples that computes Fibonacci numbers in background

fn main() -> Result<(), String> {

    gtk::init().map_err(|_| "Failed to initialize Gtk+".to_string())?;

    // Constuct new executor
    let gtk_executor = GtkEventLoopAsyncExecutor::new();
    // This examples uses CPU pool for invoking long-running computation in background
    let cpu_pool = CpuPool::new_num_cpus();

    let fut_main = gui_main(cpu_pool.clone(), gtk_executor.clone())
        .then(|_| {
            // Exit main loop when gui_main() finishes
            gtk::main_quit();

            future::ok(())
        });

    // This executes the async main function inside Gtk+ event loop
    gtk_executor.spawn(fut_main);

    gtk::main();

    Result::Ok(())
}

// An async function that shows a window. Returned future will resolve when user closes the window.
fn gui_main(cpu_pool: CpuPool, gtk_executor: GtkEventLoopAsyncExecutor) -> impl Future<Item=(), Error=String> {

    let promise = Promise::new();

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let label = gtk::Label::new("Enter n:");
    let result_label = gtk::Label::new("<result>");
    let textbox = gtk::Entry::new();
    let button = gtk::Button::new_with_label("OK");

    window.add(&vbox);
    vbox.pack_start(&label, false, true, 0);
    vbox.pack_start(&textbox, false, true, 0);
    vbox.pack_start(&button, false, true, 0);
    vbox.pack_start(&result_label, false, true, 0);

    window.set_title("Fib");
    window.set_position(gtk::WindowPosition::Center);

    {
        let promise = promise.clone();
        window.connect_delete_event(move |_, _| {
            promise.resolve(());

            Inhibit(false)
        });
    }

    {
        let cpu_pool = cpu_pool.clone();
        let gtk_executor = gtk_executor.clone();
        let textbox = textbox.clone();
        let result_label = result_label.clone();
        button.connect_clicked(move |_| {

            let opt_text = textbox.get_text();
            let text = opt_text.as_ref().map(|s| s.as_str()).unwrap_or("");
            let n: u64 = match text.parse() {
                Ok(x) => x,
                Err(x) => {
                    eprintln!("Error: {}", x);
                    return;
                }
            };
            result_label.set_text("computing...");
            let result_label = result_label.clone();

            // With GtkEventLoopAsyncExecutor we can await the long running async computation
            // and continue manipulating GUI widgets on the main thread.
            gtk_executor.spawn(
                // cpu_pool execute `compute_fib` in background thread_pool
                cpu_pool.spawn_fn(move || future::ok(compute_fib(n)))
                    .and_then(move |r| {
                        // this code is executed on main thread
                        result_label.set_text(&format!("fib({}) = {}", n, r));

                        future::ok(())
                    })
            );
        });
    }

    window.show_all();

    promise
}

// Fibonacci function. This function will take very long time for large values of `n`.
fn compute_fib(n: u64) -> u64 {
    if n < 2 {
        1
    } else {
        compute_fib(n - 2) + compute_fib(n - 1)
    }
}
```