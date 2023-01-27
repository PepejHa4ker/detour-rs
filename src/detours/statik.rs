use crate::error::{Error, Result};
use crate::{Function, GenericDetour};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::{mem, ptr};
use std::marker::Tuple;

/// A type-safe static detour.
///
/// Due to being generated by a macro, the `StaticDetour::call` method is not
/// exposed in the documentation.  
///
/// ```c
/// /// Calls the original function regardless of whether it's hooked or not.
/// ///
/// /// Panics if called when the static detour has not yet been initialized.
/// fn call(&self, T::Arguments) -> T::Output
/// ```
///
/// To define a static detour, use the
/// [static_detour](./macro.static_detour.html) macro.
///
/// # Example
///
/// ```rust
/// use std::error::Error;
/// use detour::static_detour;
///
/// static_detour! {
///   static Test: fn(i32) -> i32;
/// }
///
/// fn add5(val: i32) -> i32 {
///   val + 5
/// }
///
/// fn add10(val: i32) -> i32 {
///   val + 10
/// }
///
/// fn main() -> Result<(), Box<dyn Error>> {
///   // Replace the 'add5' function with 'add10' (can also be a closure)
///   unsafe { Test.initialize(add5, add10)? };
///
///   assert_eq!(add5(1), 6);
///   assert_eq!(Test.call(1), 6);
///
///   unsafe { Test.enable()? };
///
///   // The original function is detoured to 'add10'
///   assert_eq!(add5(1), 11);
///
///  // The original function can still be invoked using 'call'
///   assert_eq!(Test.call(1), 6);
///
///   // It is also possible to change the detour whilst hooked
///   Test.set_detour(|val| val - 5);
///   assert_eq!(add5(5), 0);
///
///   unsafe { Test.disable()? };
///
///   assert_eq!(add5(1), 6);
///   Ok(())
/// }
/// ```
pub struct StaticDetour<T: Function> {
  closure: AtomicPtr<Box<dyn Fn<T::Arguments, Output = T::Output>>>,
  detour: AtomicPtr<GenericDetour<T>>,
  ffi: T,
}

impl<T: Function> StaticDetour<T> {
  /// Create a new static detour.
  #[doc(hidden)]
  pub const fn __new(ffi: T) -> Self {
    StaticDetour {
      closure: AtomicPtr::new(ptr::null_mut()),
      detour: AtomicPtr::new(ptr::null_mut()),
      ffi,
    }
  }

  /// Create a new hook given a target function and a compatible detour
  /// closure.
  ///
  /// This method can only be called once per static instance. Multiple calls
  /// will error with `AlreadyExisting`.
  ///
  /// It returns `&self` to allow chaining initialization and activation:
  ///
  /// ```rust
  /// # use detour::{Result, static_detour};
  /// # static_detour! {
  /// #   static Test: fn(i32) -> i32;
  /// # }
  /// #
  /// # fn add5(val: i32) -> i32 {
  /// #   val + 5
  /// # }
  /// #
  /// # fn main() -> Result<()> {
  /// unsafe { Test.initialize(add5, |x| x - 5)?.enable()? };
  /// # Ok(())
  /// # }
  /// ```
  pub unsafe fn initialize<D>(&self, target: T, closure: D) -> Result<&Self>
  where
    D: Fn<T::Arguments, Output = T::Output> + Send + 'static, <T as Function>::Arguments: Tuple
  {
    let mut detour = Box::new(GenericDetour::new(target, self.ffi)?);
    if self
      .detour
      .compare_exchange(
        ptr::null_mut(),
        &mut *detour,
        Ordering::SeqCst,
        Ordering::SeqCst,
      )
      .is_err()
    {
      Err(Error::AlreadyInitialized)?;
    }

    self.set_detour(closure);
    mem::forget(detour);
    Ok(self)
  }

  /// Enables the detour.
  pub unsafe fn enable(&self) -> Result<()> {
    self
      .detour
      .load(Ordering::SeqCst)
      .as_ref()
      .ok_or(Error::NotInitialized)?
      .enable()
  }

  /// Disables the detour.
  pub unsafe fn disable(&self) -> Result<()> {
    self
      .detour
      .load(Ordering::SeqCst)
      .as_ref()
      .ok_or(Error::NotInitialized)?
      .disable()
  }

  /// Returns whether the detour is enabled or not.
  pub fn is_enabled(&self) -> bool {
    unsafe { self.detour.load(Ordering::SeqCst).as_ref() }
      .map(|detour| detour.is_enabled())
      .unwrap_or(false)
  }

  /// Changes the detour, regardless of whether the hook is enabled or not.
  pub fn set_detour<C>(&self, closure: C)
  where
    C: Fn<T::Arguments, Output = T::Output> + Send + 'static, <T as Function>::Arguments: Tuple
  {
    let previous = self
      .closure
      .swap(Box::into_raw(Box::new(Box::new(closure))), Ordering::SeqCst);
    if !previous.is_null() {
      mem::drop(unsafe { Box::from_raw(previous) });
    }
  }

  /// Returns a reference to the generated trampoline.
  pub(crate) fn trampoline(&self) -> Result<&()> {
    Ok(
      unsafe { self.detour.load(Ordering::SeqCst).as_ref() }
        .ok_or(Error::NotInitialized)?
        .trampoline(),
    )
  }

  /// Returns a transient reference to the active detour.
  #[doc(hidden)]
  pub fn __detour(&self) -> &dyn Fn<T::Arguments, Output = T::Output> {
    // TODO: This is not 100% thread-safe in case the thread is stopped
    unsafe { self.closure.load(Ordering::SeqCst).as_ref() }
      .ok_or(Error::NotInitialized)
      .expect("retrieving detour closure")
  }
}

impl<T: Function> Drop for StaticDetour<T> {
  fn drop(&mut self) {
    let previous = self.closure.swap(ptr::null_mut(), Ordering::Relaxed);
    if !previous.is_null() {
      mem::drop(unsafe { Box::from_raw(previous) });
    }

    let previous = self.detour.swap(ptr::null_mut(), Ordering::Relaxed);
    if !previous.is_null() {
      unsafe { Box::from_raw(previous) };
    }
  }
}
