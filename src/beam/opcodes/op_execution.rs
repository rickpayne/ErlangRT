//! Module implements opcodes related to execution control: Calls, jumps,
//! returns etc.

use beam::gen_op;
use beam::opcodes::assert_arity;
use beam::vm_loop::DispatchResult;
use emulator::code::CodePtr;
use emulator::process::Process;
use emulator::runtime_ctx::{Context, call_bif};
use rt_defs::stack::IStack;
use rt_defs::{ExceptionType};
use term::builders::{make_badfun, make_badmatch};
use term::lterm::*;
use term::raw::ho_import::HOImport;


fn module() -> &'static str { "opcodes::op_execution: " }


/// Perform a call to a `location` in code, storing address of the next opcode
/// in `ctx.cp`.
#[inline]
pub fn opcode_call(ctx: &mut Context,
                   _curr_p: &mut Process) -> DispatchResult {
  // Structure: call(arity:int, loc:CP)
  assert_arity(gen_op::OPCODE_CALL, 2);

  let arity = ctx.fetch_term();
  ctx.live = arity.small_get_u();

  let location = ctx.fetch_term();
  debug_assert!(location.is_box(),
                "Call location must be a box (have {})", location);

  ctx.cp = ctx.ip; // Points at the next opcode after this
  ctx.ip = CodePtr::from_cp(location);

  DispatchResult::Normal
}


/// Perform a call to a `location` in code, the `ctx.cp` is not updated.
/// Behaves like a jump?
#[inline]
pub fn opcode_call_only(ctx: &mut Context,
                        _curr_p: &mut Process) -> DispatchResult {
  // Structure: call_only(arity:int, loc:cp)
  assert_arity(gen_op::OPCODE_CALL_ONLY, 2);

  let arity = ctx.fetch_term();
  ctx.live = arity.small_get_u();

  let location = ctx.fetch_term();
  debug_assert!(location.is_box(),
                "Call location must be a box (have {})", location);

  ctx.ip = CodePtr::from_cp(location);

  DispatchResult::Normal
}


/// Performs a tail recursive call to a Destination mfarity (a `HOImport`
/// object on the heap which contains `Mod`, `Fun`, and  `Arity`) which can
/// point to an external function or a BIF. Does not update the `ctx.cp`.
#[inline]
pub fn opcode_call_ext_only(ctx: &mut Context,
                            curr_p: &mut Process) -> DispatchResult {
  // Structure: call_ext_only(arity:int, import:boxed)
  assert_arity(gen_op::OPCODE_CALL_EXT_ONLY, 2);
  shared_call_ext(ctx, curr_p,false)
}


/// Performs a call to a Destination mfarity (a `HOImport` object on the heap
/// which contains `Mod`, `Fun`, and  `Arity`) which can point to an external
/// function or a BIF. Updates the `ctx.cp` with return IP.
#[inline]
pub fn opcode_call_ext(ctx: &mut Context,
                       curr_p: &mut Process) -> DispatchResult {
  // Structure: call_ext(arity:int, destination:boxed)
  assert_arity(gen_op::OPCODE_CALL_EXT, 2);
  shared_call_ext(ctx, curr_p, true)
}


#[inline]
fn shared_call_ext(ctx: &mut Context,
                   curr_p: &mut Process,
                   save_cp: bool) -> DispatchResult {
  let arity = ctx.fetch_term().small_get_u();
  ctx.live = arity;

  // HOImport object on heap which contains m:f/arity
  let imp0 = ctx.fetch_term();

  match unsafe { HOImport::from_term(imp0) } {
    Ok(import) =>
      unsafe {
        if (*import).is_bif {
          // Perform a BIF application
          //
          return call_bif(ctx, curr_p, arity, true)
        } else {
          // Perform a regular call to BEAM code, save CP and jump
          //
          if save_cp {
            ctx.cp = ctx.ip; // Points at the next opcode after this
          }
          ctx.ip = (*import).resolve().unwrap();
          return DispatchResult::Normal
        }
      },
    Err(err) => {
      // Create a `{badfun, _}` error
      let badfun = make_badfun(imp0, &mut curr_p.heap);
      return DispatchResult::Error(ExceptionType::Error, badfun)
    }
  }
}


/// Jump to the value in `ctx.cp`, set `ctx.cp` to NULL. Empty stack means that
/// the process has no more code to execute and will end with reason `normal`.
#[inline]
pub fn opcode_return(ctx: &mut Context,
                     curr_p: &mut Process) -> DispatchResult {
  // Structure: return()
  assert_arity(gen_op::OPCODE_RETURN, 0);

  if ctx.cp.is_null() {
    if curr_p.heap.stack_depth() == 0 {
      // Process end of life: return on empty stack
      panic!("{}Process exit: normal; x0={}", module(), ctx.regs[0])
    } else {
      panic!("{}Return instruction with 0 in ctx.cp", module())
    }
  }

  ctx.ip = ctx.cp;
  ctx.cp = CodePtr::null();

  DispatchResult::Normal
}


#[inline]
pub fn opcode_func_info(ctx: &mut Context, _curr_p: &mut Process) -> DispatchResult {
  assert_arity(gen_op::OPCODE_FUNC_INFO, 3);
  let m = ctx.fetch_term();
  let f = ctx.fetch_term();
  let arity = ctx.fetch_term();

  panic!("{}function_clause {}:{}/{}", module(), m, f, arity)
  //DispatchResult::Error
}


/// Create an error:badmatch exception
#[inline]
pub fn opcode_badmatch(ctx: &mut Context,
                       curr_p: &mut Process) -> DispatchResult {
  // Structure: badmatch(LTerm)
  assert_arity(gen_op::OPCODE_BADMATCH, 1);

  let hp = &mut curr_p.heap;
  let val = ctx.fetch_and_load(hp);
  let badmatch = make_badmatch(val, hp);
  DispatchResult::Error(ExceptionType::Error, badmatch)
}
