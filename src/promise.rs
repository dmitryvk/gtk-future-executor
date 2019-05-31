use futures::prelude::*;

#[derive(Debug)]
struct PromiseBackend<T, E> {
    result: Option<Result<T, E>>,
    waiting_tasks: Vec<futures::task::Task>,
}

/// `Promise` is a future that can be completed or failed with `resolve` or `reject` methods.
/// 
/// `Promise` object is freely cloneable (all clones refer to the same underlying object) and is thread-safe.
/// 
/// `Promise` objects are handy for integrating `Future`-based code with non-`Future` based code.
/// 
/// Example:
/// ```rust
/// // A function that shows the window;
/// // returned future that will be resolved when the windows is closed.
/// fn gui_main() -> impl Future<Item=(), Error=()> {
///     let promise = Promise::new();
/// 
///     let window = gtk::Window::new(gtk::WindowType::TopLevel);
/// 
///     {
///         // Make a clone of promise since it will be moved into closure
///         let promise = promise.clone();
///         window.connect_delete_event(move |_,_| {
///             // When window is closed the promise will be resolved
///             promise.resolve(());
/// 
///             Inhibit(false)
///         });
///     }
/// 
///     window.show_all();
/// 
///     promise
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Promise<T, E> {
    backend: std::sync::Arc<std::sync::Mutex<PromiseBackend<T, E>>>,
}

impl<T, E> Promise<T, E> {
    /// Construct a new promise
    pub fn new() -> Promise<T, E> {
        Promise {
            backend: std::sync::Arc::new(
                std::sync::Mutex::new(
                    PromiseBackend {
                        result: None,
                        waiting_tasks: vec![],
                    }
                )
            )
        }
    }

    /// Complete the promise with specified value.
    /// Once this method is called, no further calls to `resolve()` or `reject()` should be made.
    pub fn resolve(&self, result: T) {
        let mut backend = self.backend.lock().unwrap();

        backend.result = Some(Ok(result));
        for task in &backend.waiting_tasks {
            task.notify();
        }

        backend.waiting_tasks.clear();
    }

    /// Complete the promise with specified error.
    /// Once this method is called, no further calls to `resolve()` or `reject()` should be made.
    pub fn reject(&self, error: E) {
        let mut backend = self.backend.lock().unwrap();

        backend.result = Some(Err(error));
        for task in &backend.waiting_tasks {
            task.notify();
        }

        backend.waiting_tasks.clear();
    }
}

impl <T, E> Future for Promise<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut guard = self.backend.lock().unwrap();
        let backend = &mut *guard;

        match backend.result {
            Some(_) => match backend.result.take().unwrap() {
                Ok(result) => std::result::Result::Ok(Async::Ready(result)),
                Err(error) => std::result::Result::Err(error),
            },
            None => {
                backend.waiting_tasks.push(futures::task::current());
                std::result::Result::Ok(Async::NotReady)
            }
        }
    }
}
