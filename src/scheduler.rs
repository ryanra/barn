#![allow(dead_code)]

use core::mem::{transmute};

use super::fringe_wrapper::Group;

pub struct Thread<U: Unit> {
  group: Group<'static, ThreadResponse<U>, ThreadRequest<U>, U::S>,
  name: &'static str,
  local: U::L,
}

unsafe impl<U: Unit> Send for Thread<U> {}

impl<U: Unit> ::core::ops::Deref for Thread<U> {
    type Target = U::L;

    fn deref(&self) -> &U::L {
        &self.local
    }
}



impl<U: Unit> Thread<U> {
  
  pub fn new<F>(f: F, stack: U::S, name: &'static str) -> Thread<U>  where F: FnOnce() + Send + 'static {
    Thread {
      group: unsafe { Group::new(f, stack) },
      name: name,
      local: U::L::default(),
    }
  }
  
  pub fn name(&self) -> &'static str {
    self.name
  }
  
  unsafe fn request(&mut self, request: ThreadRequest<U>) -> ThreadResponse<U> {
    self.group.suspend(request)
  }

}

pub trait Node<U: Unit> where Self: Send + Sized {
  
  fn new(t: Thread<U>) -> Self where Self: Sized;
  
  // TODO: should be able to inhereit associated type, but looks like compiler problem.
  fn deref(&self) -> &Thread<U>;
  
  fn deref_mut(&mut self) -> &mut Thread<U>;
}

pub trait Stack: ::fringe::Stack + Sized {
  fn new(size: usize) -> Self;
}

// Must do no allocations for these methods
pub trait Queue<U: Unit> where Self: Sized + Sync + 'static {
  
  fn push(&mut self, node: U::N);
  
  fn pop(&mut self) -> Option<U::N>;
  
  fn front(&self) -> Option<&U::N>;

  fn front_mut(&mut self) -> Option<&mut U::N>;
}

pub trait Unit: 'static + Sized + Sync {
  type L: Default;
  type S: Stack;
  type N: Node<Self>;
}

// Request of thread to scheduler
enum ThreadRequest<U: Unit> {
    Yield,
    StageUnschedule,
    // idea, instead of donating a node, donate an abstract
    // "donation" if determine that a run queue is low
    Schedule(U::N),
    CompleteUnschedule,
}

// Response
enum ThreadResponse<U: Unit> {
    Nothing,
    Unscheduled(U::N)
}

pub struct Scheduler<U: Unit, Q: Queue<U>> {
    queue: Q,
    _phantom: ::core::marker::PhantomData<U>, // complains the U is not used but is used for type arg of Q...
}

impl<U: Unit, Q: Queue<U>> Scheduler<U, Q> {
  
  // Creates a scheduler with the given thread queue
  pub fn new(queue: Q) -> Scheduler<U, Q> {
    Scheduler { queue: queue, _phantom: ::core::marker::PhantomData }
  }
  
  fn current_thread(&self) -> &Thread<U> {
    self.queue.front().unwrap().deref()
  }
  
  fn current_thread_mut(&mut self) -> &mut Thread<U> {
    self.queue.front_mut().unwrap().deref_mut()
  }
  
  fn next_request(&mut self, response: ThreadResponse<U>) -> Option<Option<ThreadRequest<U>>> {
    unsafe {
      self.queue.front_mut().map(|x| x.deref_mut().group.resume(response))
    }
  }
  
  pub fn run(&mut self) {
    let mut response = ThreadResponse::Nothing;
    
    while let Some(request) = self.next_request(response) {
        response = match request {
            Some(req) => match req {
                ThreadRequest::Yield => {
                    let c = self.queue.pop().unwrap();
                    self.queue.push(c);
                    ThreadResponse::Nothing
                },
                ThreadRequest::StageUnschedule => {
                    // We have to pass `node` to a resume call on the tcb in node.
                    // To do so, we need to get around the borrow checker.
                    let node: &U::N = self.queue.front().unwrap();
                    let static_node: *const U::N = unsafe { transmute(node) };                        
                    unsafe { ThreadResponse::Unscheduled(::core::ptr::read(static_node)) }
                },
                ThreadRequest::CompleteUnschedule => {
                    // We can assert that last response was unscheduled
                    // Finish unscheduling. Thread's ownership has already been passed
                    ::core::mem::forget(self.queue.pop());
                    ThreadResponse::Nothing
                },
                ThreadRequest::Schedule(tcb_node) => {
                    self.queue.push(tcb_node);
                    ThreadResponse::Nothing
                },
            },
            None => {
                // Thread is finished.
                if let Some(node) = self.queue.pop() {
                  ThreadResponse::Unscheduled(node) //don't drop in the scheduler!
                } else {
                  ThreadResponse::Nothing
                }
            },
        }
    }
  }

}
