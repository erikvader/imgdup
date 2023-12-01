use std::{any::Any, fmt, thread};

// NOTE: i'm a little unsure about all lifetimes. I've used `'scope` and `'env` in the
// same way as `thead::scope` does. I've used `'work_scope` on this struct itself since it
// didn't like to be borrowed for `'scope`. But everything seems to work as is hopefully
// correct.
pub struct WorkerScope<'scope, 'env, T> {
    inner: &'scope thread::Scope<'scope, 'env>,
    handles: Vec<(String, thread::ScopedJoinHandle<'scope, T>)>,
}

impl<'work_scope, 'scope, 'env, T> WorkerScope<'scope, 'env, T> {
    pub fn spawn<F>(&'work_scope mut self, name: impl AsRef<str>, f: F)
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        let name = name.as_ref();
        let index = self.num_spawned();
        let name = format!("{name}{index:>02}");
        let handle = thread::Builder::new()
            .name(name.clone())
            .spawn_scoped(self.inner, f)
            .expect("the name does not contain null bytes");
        self.handles.push((name, handle));
    }

    pub fn num_spawned(&self) -> usize {
        self.handles.len()
    }
}

pub struct CaughtPanic(pub Box<dyn Any + Send + 'static>);

pub struct FinishedWorker<T> {
    pub name: String,
    pub result: Result<T, CaughtPanic>,
}

pub fn scoped_workers<'env, F, T>(f: F) -> Vec<FinishedWorker<T>>
where
    F: for<'scope, 'work_scope> FnOnce(&'work_scope mut WorkerScope<'scope, 'env, T>),
{
    thread::scope(|scope| {
        let mut scope = WorkerScope {
            inner: scope,
            handles: vec![],
        };
        // TODO: catch_unwind f? or rely on thread::scope handling that?
        f(&mut scope);
        scope
            .handles
            .into_iter()
            .map(|(name, handle)| FinishedWorker {
                name,
                result: handle.join().map_err(|panic| CaughtPanic(panic)),
            })
            .collect()
    })
}

impl fmt::Display for CaughtPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let panic = &self.0;
        let string = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                format!("non-string panic message: {:?}", panic.type_id())
            });
        write!(f, "{string}")
    }
}
