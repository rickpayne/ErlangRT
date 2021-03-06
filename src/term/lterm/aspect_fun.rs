//! Functions to manipulate an LTerm as an boxed pointer to an Erlang Fun.
//! Part of LTerm impl.

//use rt_defs::Word;
//use term::immediate;


pub trait FunAspect {
  /// Check whether a value is a boxed fun (a closure).
  fn is_fun(&self) -> bool { false }
}


impl FunAspect for super::LTerm {
}
