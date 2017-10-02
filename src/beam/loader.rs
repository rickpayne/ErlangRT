//!
//! Code loader for BEAM files uses 3 stage approach.
//! Stage 1 reads the BEAM file and fills the loader state structure.
//! Stage 2 commits changes to the VM (atom table for example)
//! Stage 3 (finalize) returns Erlang module object ready for code server.
//!
//! Call `let l = Loader::new()`, then `l.load(filename)`, then
//! `l.load_stage2(&mut vm)` and finally `let modp = l.load_finalize()`
//!
use bytes::Bytes;
use std::path::PathBuf;

use mfa::Arity;
use module;
use rterror;
use types::{Word, Integral};
use util::reader;
use vm::VM;
use beam::compact_term;
use term::Term;

pub fn module() -> &'static str { "BEAM loader: " }

/// Raw data structure as loaded from BEAM file
struct LImport {
  mod_atom: u32,
  fun_atom: u32,
  arity: Arity,
}

/// Raw data structure as loaded from BEAM file
struct LExport {
  fun_atom: u32,
  arity: Arity,
  label: u32,
}

/// Raw data structure as loaded from BEAM file
struct LFun {
  fun_atom: u32,
  arity: u32,
  code_pos: u32,
  index: u32,
  nfree: u32,
  ouniq: u32,
}

pub struct Loader {
  /// Atoms as loaded from BEAM module strings.
  atom_tab: Vec<String>,
  /// Atoms converted to VM terms (during the stage 2)
  vm_atoms: Vec<Term>,
  imports: Vec<LImport>,
  exports: Vec<LExport>,
  locals: Vec<LExport>,
  funs: Vec<LFun>,
  mod_name: Term,
}

impl Loader {
  /// Construct a new loader state.
  pub fn new() -> Loader {
    Loader {
      atom_tab: Vec::new(),
      vm_atoms: Vec::new(),
      imports: Vec::new(),
      exports: Vec::new(),
      locals: Vec::new(),
      funs: Vec::new(),
      mod_name: Term::non_value(),
    }
  }

  /// Loading the module. Validate the header and iterate over sections,
  /// then call `load_stage2()` to apply changes to the VM, and then finalize
  /// it by calling `load_finalize()` which will return you a module object.
  pub fn load(&mut self, fname: &PathBuf) -> Result<(), rterror::Error>
  {
    let mut r = reader::BinaryReader::from_file(fname);

    // Parse header and check file FOR1 signature
    let hdr1 = Bytes::from(&b"FOR1"[..]);
    r.ensure_bytes(&hdr1)?;

    let beam_sz = r.read_u32be();

    // Check BEAM signature
    let hdr2 = Bytes::from(&b"BEAM"[..]);
    r.ensure_bytes(&hdr2)?;

    while true {
      // EOF may strike here when we finished reading
      let chunk_h = match r.read_str_latin1(4) {
        Ok(s) => s,
        // EOF is not an error
        Err(rterror::Error::CodeLoadingPrematureEOF) => break,
        Err(e) => return Err(e)
      };
      let chunk_sz = r.read_u32be();

      println!("Chunk {}", chunk_h);
      match chunk_h.as_ref() {
        "Atom" => self.load_atoms_latin1(&mut r),
        "Attr" => r.skip(chunk_sz as Word), // TODO: read attributes
        "AtU8" => self.load_atoms_utf8(&mut r),
        "CInf" => r.skip(chunk_sz as Word),
        "Code" => self.load_code(&mut r, chunk_sz as Word),
        "Dbgi" => r.skip(chunk_sz as Word),
        "ExpT" => self.exports = self.load_exports(&mut r),
        "FunT" => self.load_fun_table(&mut r),
        "ImpT" => self.load_imports(&mut r),
        "Line" => self.load_line_info(&mut r),
        "LocT" => self.locals = self.load_exports(&mut r),
        "StrT" => r.skip(chunk_sz as Word),
        other => {
          let msg = format!("{}Unexpected chunk: {}", module(), other);
          return Err(rterror::Error::CodeLoadingFailed(msg))
        }
      }

      // The next chunk is aligned at 4 bytes
      let aligned_sz = 4 * ((chunk_sz + 3) / 4);
      let align = aligned_sz - chunk_sz;
      if align > 0 { r.skip(align as Word); }
    }
    Ok(())
  }

  /// Call this to apply changes to the VM after module loading succeeded. The
  /// module object is not created yet, but some effects like atoms table
  /// we can already apply.
  pub fn load_stage2(&mut self, vm: &mut VM) {
    self.vm_atoms.reserve(self.atom_tab.len());
    for a in &self.atom_tab {
      self.vm_atoms.push(vm.atom(&a));
    }

    self.mod_name = self.vm_atoms[0];
  }

  /// At this point loading is finished, and we create Erlang module and
  /// return a reference counted pointer to it. VM (the caller) is responsible
  /// for adding the module to its code registry.
  pub fn load_finalize(&mut self) -> Result<module::Ptr, rterror::Error> {
    let newmod = module::Module::new(self.mod_name);
    Ok(newmod)
  }

  //============================================================================

  /// Approaching AtU8 section, populate atoms table in the Loader state.
  /// The format is: "Atom"|"AtU8", u32/big count { u8 length, "atomname" }.
  /// Formats are absolutely compatible except that Atom is latin-1
  fn load_atoms_utf8(&mut self, r: &mut reader::BinaryReader) {
    let n_atoms = r.read_u32be();
    for i in 0..n_atoms {
      let atom_bytes = r.read_u8();
      let atom_text = r.read_str_utf8(atom_bytes as Word).unwrap();
      self.atom_tab.push(atom_text);
    }
  }

  /// Approaching Atom section, populate atoms table in the Loader state.
  /// The format is: "Atom"|"AtU8", u32/big count { u8 length, "atomname" }.
  /// Same as `load_atoms_utf8` but interprets strings per-character as latin-1
  fn load_atoms_latin1(&mut self, r: &mut reader::BinaryReader) {
    let n_atoms = r.read_u32be();
    self.atom_tab.reserve(n_atoms as usize);
    for i in 0..n_atoms {
      let atom_bytes = r.read_u8();
      let atom_text = r.read_str_latin1(atom_bytes as Word).unwrap();
      self.atom_tab.push(atom_text);
    }
  }

  /// Load the `Code` section
  fn load_code(&mut self, r: &mut reader::BinaryReader, chunk_sz: Word) {
    let code_ver = r.read_u32be();
    let min_opcode = r.read_u32be();
    let max_opcode = r.read_u32be();
    let n_labels = r.read_u32be();
    let n_funs = r.read_u32be();
    println!("Code section version {}, opcodes {}-{}, labels: {}, funs: {}",
      code_ver, min_opcode, max_opcode, n_labels, n_funs);
    let code = r.read_bytes(chunk_sz - 20).unwrap();
  }

  /// Read the imports table.
  /// Format is u32/big count { modindex: u32, funindex: u32, arity: u32 }
  fn load_imports(&mut self, r: &mut reader::BinaryReader) {
    let n_imports = r.read_u32be();
    self.imports.reserve(n_imports as usize);
    for i in 0..n_imports {
      let imp = LImport {
        mod_atom: r.read_u32be(),
        fun_atom: r.read_u32be(),
        arity: r.read_u32be() as Arity,
      };
      self.imports.push(imp);
    }
  }

  /// Read the exports or local functions table (same format).
  /// Format is u32/big count { funindex: u32, arity: u32, label: u32 }
  fn load_exports(&mut self, r: &mut reader::BinaryReader) -> Vec<LExport> {
    let n_exports = r.read_u32be();
    let mut exports = Vec::new();
    exports.reserve(n_exports as usize);
    for i in 0..n_exports {
      let exp = LExport {
        fun_atom: r.read_u32be(),
        arity: r.read_u32be() as Arity,
        label: r.read_u32be(),
      };
      exports.push(exp);
    }
    exports
  }

  fn load_fun_table(&mut self, r: &mut reader::BinaryReader) {
    let n_funs = r.read_u32be();
    self.funs.reserve(n_funs as usize);
    for i in 0..n_funs {
      let fun_atom = r.read_u32be();
      let arity = r.read_u32be();
      let code_pos = r.read_u32be();
      let index = r.read_u32be();
      let nfree = r.read_u32be();
      let ouniq = r.read_u32be();
      self.funs.push(LFun {
        fun_atom, arity, code_pos, index, nfree, ouniq
      })
    }
  }

  fn load_line_info(&mut self, r: &mut reader::BinaryReader) {
    let version = r.read_u32be(); // must match emulator version 0
    let flags = r.read_u32be();
    let n_line_instr = r.read_u32be();
    let n_line_refs = r.read_u32be();
    let n_filenames = r.read_u32be();
    let mut fname_index = 0u32;

    for i in 0..n_line_refs {
      match compact_term::read(r).unwrap() {
        compact_term::CompactTerm::Integer(Integral::Word(w)) => {
          // self.linerefs.push((fname_index, w));
        },
        compact_term::CompactTerm::Atom(a) =>
          fname_index = a as u32,
        other => panic!("{}Unexpected data in line info section: {:?}",
                        module(), other)
      }
    }

    for i in 0..n_filenames {
      let name_size = r.read_u16be();
      let fstr = r.read_str_utf8(name_size as Word);
    }
  }

}