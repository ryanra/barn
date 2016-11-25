use core::ops::Deref;
use core::ops::DerefMut;
use core::marker::PhantomData;
use core::cell::UnsafeCell;

use scheduler::{Scheduler, Thread, Request, SchedulerUnit, Queue};

pub struct Mutex<T, U: SchedulerUnit> {
  queue_lock: ::spin::Mutex<(U::Q, bool)>,
  data: UnsafeCell<T>,
  p: PhantomData<U>
}

pub struct MutexGuard<'a, T:'a, U: SchedulerUnit> {
    lock: &'a Mutex<T, U>
}

impl<'a, T: 'a, U: SchedulerUnit> MutexGuard<'a, T, U> {

  fn new(lock: &'a Mutex<T, U>) -> MutexGuard<'a, T, U> {
    MutexGuard { lock: lock }
  }

}

impl<'a, T:'a, U: SchedulerUnit> Deref for MutexGuard<'a, T, U> {
  type Target = T;

  fn deref(&self) -> &T {
    unsafe { &*self.lock.data.get() }
  }

}

impl<'a, T:'a, U: SchedulerUnit> DerefMut for MutexGuard<'a, T, U> {
  fn deref_mut(&mut self) -> &mut T {
    unsafe { &mut *self.lock.data.get() }
  }
}

impl<'a, T:'a, U: SchedulerUnit> Drop for MutexGuard<'a, T, U> {

  fn drop(&mut self) {
    self.lock.unlock();
  }

}

impl<T, U: SchedulerUnit> Mutex<T, U> {

  pub fn new(data: T) -> Mutex<T, U> {
    Mutex { queue_lock: ::spin::Mutex::new((U::Q::new(), false)), data: UnsafeCell::new(data), p: PhantomData::<U> }
  }

  pub fn try_lock(&self) -> Option<MutexGuard<T, U>> {
    let mut l = self.queue_lock.lock();
    let &mut (_, ref mut taken) = l.deref_mut();
    if *taken {
      None
    } else {
      *taken = true;
      Some(MutexGuard::new(self))
    }
  }

  pub fn lock(&self) -> MutexGuard<T, U> {
    loop {
      let mut l = self.queue_lock.lock();
      match l.deref_mut() {
        &mut (_, ref mut taken) => {
          if !*taken {
            *taken = true;
            break;
          }
        }
      }
      let take = move |me| {
        match l.deref_mut() {
          &mut (ref mut queue, _) => queue.push(me)
        }
        drop(l);
      };
      Thread::<U>::suspend(Request::make_schedule(&take));
    }
    MutexGuard::new(self)
  }

  fn unlock(&self) {
    let mut l = self.queue_lock.lock();
    let &mut (ref mut queue, ref mut taken) = l.deref_mut();
    *taken = false;
    if let Some(node) = queue.pop() {
      Thread::<U>::suspend(Request::Schedule(node));
    }
  }

}


unsafe impl<T: Send, U: SchedulerUnit> Send for Mutex<T, U> { }
unsafe impl<T: Send, U: SchedulerUnit> Sync for Mutex<T, U> { }