pub use self::patcher::Patcher;
pub use self::trampoline::Trampoline;

pub mod meta;
mod patcher;
mod thunk;
mod trampoline;

// TODO: Add test for targets further away than DETOUR_RANGE
// TODO: Add test for unsupported branches
// TODO: Add test for negative branch displacements
#[cfg(all(feature = "nightly", test))]
mod tests {
  use std::arch::asm;
  use crate::error::{Error, Result};
  use crate::RawDetour;
  use matches::assert_matches;
  use std::mem;

  /// Default test case function definition.
  type CRet = unsafe extern "C" fn() -> i32;

  /// Detours a C function returning an integer, and asserts its return value.
  #[inline(never)]
  unsafe fn detour_test(target: CRet, result: i32) -> Result<()> {
    let hook = RawDetour::new(target as *const (), ret10 as *const ())?;

    assert_eq!(target(), result);
    hook.enable()?;
    {
      assert_eq!(target(), 10);
      let original: CRet = mem::transmute(hook.trampoline());
      assert_eq!(original(), result);
    }
    hook.disable()?;
    assert_eq!(target(), result);
    Ok(())
  }



  #[test]
  fn detour_hotpatch() -> Result<()> {
    #[naked]
    unsafe extern "C" fn hotpatch_ret0() -> i32 {
      asm!(
        "
            nop
            nop
            nop
            nop
            nop
            xor eax, eax
            ret
            mov eax, 5",
        options(noreturn)
      )
    }

    unsafe { detour_test(mem::transmute(hotpatch_ret0 as usize + 5), 0) }
  }





  #[test]
  #[cfg(target_arch = "x86_64")]
  fn detour_rip_relative_pos() -> Result<()> {
    #[naked]
    unsafe extern "C" fn rip_relative_ret195() -> i32 {
      asm!(
        "
            xor eax, eax
            mov al, [rip+0x3]
            nop
            nop
            nop
            ret",
        options(noreturn)
      )
    }

    unsafe { detour_test(rip_relative_ret195, 195) }
  }

  #[test]
  #[cfg(target_arch = "x86_64")]
  fn detour_rip_relative_neg() -> Result<()> {
    #[naked]
    unsafe extern "C" fn rip_relative_prolog_ret49() -> i32 {
      asm!(
        "
            xor eax, eax
            mov al, [rip-0x8]
            ret",
        options(noreturn)
      )
    }

    unsafe { detour_test(rip_relative_prolog_ret49, 49) }
  }

  /// Default detour target.
  unsafe extern "C" fn ret10() -> i32 {
    10
  }
}
