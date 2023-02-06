use std::{
    cell::{RefCell, RefMut},
    error::Error,
    fmt::Debug,
    rc::Rc,
};

pub trait Action: Debug {
    fn name(&self) -> Option<String>;
    fn run(self: Box<Self>) -> Result<(), Box<dyn Error>>;
    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef>;
    fn progress_report(&self) -> bool {
        true
    }
}

impl<T: Action + 'static> From<T> for ActionRef {
    fn from(value: T) -> Self {
        ActionRef::new(value)
    }
}

#[derive(Debug, Clone)]
pub struct ActionRef {
    inner: Rc<RefCell<Option<Box<dyn Action>>>>,
}

impl ActionRef {
    pub fn new<T: Action + 'static>(action: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Some(Box::new(action)))),
        }
    }

    pub fn get_mut(&self) -> RefMut<Option<Box<dyn Action>>> {
        self.inner.borrow_mut()
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        if let Some(action) = self.inner.borrow_mut().take() {
            action.run()?;
        }
        Ok(())
    }
}
