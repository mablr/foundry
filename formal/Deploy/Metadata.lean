/-!
# Metadata stripping for `forge verify-bytecode`

Models `find_metadata_start` and `ignore_metadata_hash` (`crates/common/src/utils.rs`) and
`match_bytecodes`, `is_partial_match` and `try_extract_and_compare_bytecode`
(`crates/verify/src/utils.rs`).

Solidity appends `M ++ u16be(|M|)` to the code, where `M` is the CBOR metadata. A *partial*
match accepts local and onchain code that differ only inside `M`. For creation code with
constructor arguments `A`, verify builds `local = code ++ A` and checks the onchain creation
code against it; a match is what validates user-provided arguments.

## Fidelity

- `cbor : List UInt8 → Bool` abstracts `ciborium::from_reader::<Value, _>(_).is_ok()`. The
  theorems hold for every such predicate. The counterexample only assumes what ciborium 0.2.2
  actually does: `0x60` (an empty text string) followed by anything decodes, because
  `from_reader` reads one item and ignores trailing bytes.
- `u16::from_be_bytes` is `u16be`.
- Slices are `List.take`/`List.drop`; the Rust indices are in bounds wherever the model uses
  them (`find_metadata_start` checks `metadata_len <= rest.len()`).
-/

namespace Deploy.Metadata

/-- Big-endian `u16` of two bytes. -/
def u16be (hi lo : UInt8) : Nat := hi.toNat * 256 + lo.toNat

theorem u16be_inj {a b c d : UInt8} (h : u16be a b = u16be c d) : a = c ∧ b = d := by
  unfold u16be at h
  have ha := a.toNat_lt; have hb := b.toNat_lt; have hc := c.toNat_lt; have hd := d.toNat_lt
  have h1 : a.toNat = c.toNat := by omega
  have h2 : b.toNat = d.toNat := by omega
  exact ⟨UInt8.toNat_inj.mp h1, UInt8.toNat_inj.mp h2⟩

variable (cbor : List UInt8 → Bool)

/-- The trailing length field of `b` (meaningful when `2 ≤ b.length`). -/
def trailingLen (b : List UInt8) : Nat :=
  u16be (b.getD (b.length - 2) 0) (b.getD (b.length - 1) 0)

/-- Model of `find_metadata_start`. -/
def findMetadataStart (b : List UInt8) : Option Nat :=
  if 2 ≤ b.length ∧ trailingLen b ≤ b.length - 2 then
    if cbor ((b.take (b.length - 2)).drop (b.length - 2 - trailingLen b)) then
      some (b.length - 2 - trailingLen b)
    else none
  else none

/-- Model of `ignore_metadata_hash`. -/
def ignoreMetadataHash (b : List UInt8) : List UInt8 :=
  match findMetadataStart cbor b with
  | some m => b.take m
  | none => b

/-- `try_extract_and_compare_bytecode` before the fix: strip each side by its own trailing
length and compare what is left. -/
def tryExtractOld (loc onchain : List UInt8) : Bool :=
  ignoreMetadataHash cbor loc == ignoreMetadataHash cbor onchain

/-- `try_extract_and_compare_bytecode` after the fix: the lengths must agree as well. -/
def tryExtract (loc onchain : List UInt8) : Bool :=
  loc.length == onchain.length && tryExtractOld cbor loc onchain

/-- Model of `is_partial_match`, parametric in the final comparison. -/
def isPartialMatchWith (cmp : List UInt8 → List UInt8 → Bool)
    (loc onchain args : List UInt8) (isRuntime : Bool) : Bool :=
  if args.isEmpty || isRuntime then cmp loc onchain
  else if !(args.isSuffixOf onchain) then false
  else if args.length ≤ onchain.length ∧ args.length ≤ loc.length then
    cmp (loc.take (loc.length - args.length)) (onchain.take (onchain.length - args.length))
  else false

/-- Outcome of a verification, `VerificationType` in Rust. -/
inductive MatchType where
  | full
  | partialMatch
  deriving DecidableEq, Repr

/-- Model of `match_bytecodes`, parametric in the final comparison. -/
def matchBytecodesWith (cmp : List UInt8 → List UInt8 → Bool)
    (loc onchain args : List UInt8) (isRuntime hashNone : Bool) : Option MatchType :=
  if loc = onchain then some (if hashNone then .partialMatch else .full)
  else if isPartialMatchWith cmp loc onchain args isRuntime then some .partialMatch
  else none

/-- `match_bytecodes` before the fix. -/
abbrev matchBytecodesOld := matchBytecodesWith (tryExtractOld cbor)

/-- `match_bytecodes` after the fix. -/
abbrev matchBytecodes := matchBytecodesWith (tryExtract cbor)

/-! ## Soundness of the fixed comparison -/

/-- Two codes of length `n` agree everywhere except possibly in the metadata region `[m, n - 2)`
that both declare with the same trailing length, and that region decodes on both sides. -/
def SameUpToMetadata (a b : List UInt8) : Prop :=
  a.length = b.length ∧
    (a = b ∨ ∃ m, findMetadataStart cbor a = some m ∧ findMetadataStart cbor b = some m ∧
      a.take m = b.take m ∧ a.drop (a.length - 2) = b.drop (b.length - 2))

theorem findMetadataStart_spec {b : List UInt8} {m : Nat}
    (h : findMetadataStart cbor b = some m) :
    2 ≤ b.length ∧ m = b.length - 2 - trailingLen b ∧ trailingLen b ≤ b.length - 2 := by
  unfold findMetadataStart at h
  split at h
  · split at h
    · simp at h; omega
    · simp at h
  · simp at h

theorem drop_two (a : List UInt8) (h : 2 ≤ a.length) :
    a.drop (a.length - 2) = [a.getD (a.length - 2) 0, a.getD (a.length - 1) 0] := by
  rw [List.drop_eq_getElem_cons (by omega), List.drop_eq_getElem_cons (by omega)]
  have e : a.length - 2 + 1 + 1 = a.length := by omega
  have e' : a.length - 1 = a.length - 2 + 1 := by omega
  rw [e, List.drop_length, e']
  simp [List.getD_eq_getElem?_getD, List.getElem?_eq_getElem (show a.length - 2 < a.length by omega),
    List.getElem?_eq_getElem (show a.length - 2 + 1 < a.length by omega)]

theorem drop_two_of_trailingLen {a b : List UInt8} (hl : a.length = b.length) (h2 : 2 ≤ a.length)
    (h : trailingLen a = trailingLen b) : a.drop (a.length - 2) = b.drop (b.length - 2) := by
  unfold trailingLen at h
  obtain ⟨h1, h2'⟩ := u16be_inj h
  rw [drop_two a h2, drop_two b (by omega), h1, h2']

/-- **Headline (comparison).** The fixed comparison accepts only codes that are equal up to a
metadata region both sides declare identically. -/
theorem tryExtract_sound {a b : List UInt8} (h : tryExtract cbor a b = true) :
    SameUpToMetadata cbor a b := by
  unfold tryExtract tryExtractOld ignoreMetadataHash at h
  simp only [Bool.and_eq_true, beq_iff_eq] at h
  obtain ⟨hl, h⟩ := h
  refine ⟨hl, ?_⟩
  revert h
  cases ha : findMetadataStart cbor a with
  | none =>
    cases hb : findMetadataStart cbor b with
    | none => intro h; exact Or.inl h
    | some mb =>
      intro h
      have := findMetadataStart_spec cbor hb
      have := congrArg List.length h
      simp at this; omega
  | some ma =>
    have sa := findMetadataStart_spec cbor ha
    cases hb : findMetadataStart cbor b with
    | none =>
      intro h
      have := congrArg List.length h
      simp at this; omega
    | some mb =>
      intro h
      have sb := findMetadataStart_spec cbor hb
      have hlen := congrArg List.length h
      simp at hlen
      have hm : ma = mb := by omega
      subst hm
      refine Or.inr ⟨ma, rfl, rfl, h, drop_two_of_trailingLen hl sa.1 (by omega)⟩

/-- The fixed comparison accepts every pair equal up to metadata, so `tryExtract` is exactly
`SameUpToMetadata`; the Rust conformance test checks this equivalence. -/
theorem tryExtract_complete {a b : List UInt8} (h : SameUpToMetadata cbor a b) :
    tryExtract cbor a b = true := by
  obtain ⟨hl, rfl | ⟨m, ha, hb, ht, _⟩⟩ := h
  · simp [tryExtract, tryExtractOld]
  · simp [tryExtract, tryExtractOld, ignoreMetadataHash, ha, hb, ht, hl]

theorem tryExtract_iff (a b : List UInt8) : tryExtract cbor a b = true ↔ SameUpToMetadata cbor a b :=
  ⟨tryExtract_sound cbor, tryExtract_complete cbor⟩

/-- **Headline (verify, creation code with arguments).** If `match_bytecodes` accepts the local
creation code `code ++ args` against the onchain creation code, then the onchain code is
`code' ++ args` with `code'` equal to `code` up to metadata of the same length. -/
theorem matchBytecodes_creation_sound {code args onchain : List UInt8} {hashNone : Bool}
    {t : MatchType} (hargs : args ≠ [])
    (h : matchBytecodes cbor (code ++ args) onchain args false hashNone = some t) :
    ∃ code', onchain = code' ++ args ∧ SameUpToMetadata cbor code code' := by
  unfold matchBytecodes matchBytecodesWith isPartialMatchWith at h
  split at h
  · rename_i he
    exact ⟨code, he.symm, rfl, Or.inl rfl⟩
  · have hne : args.isEmpty = false := by cases args <;> simp_all
    simp only [hne, Bool.or_false, Bool.false_eq_true, if_false] at h
    split at h
    · simp at h
    · rename_i hsuf
      simp only [Bool.not_eq_true', Bool.not_eq_false] at hsuf
      obtain ⟨code', hcode'⟩ := List.isSuffixOf_iff_suffix.mp hsuf
      split at h
      · split at h
        · rename_i _ hc
          subst hcode'
          refine ⟨code', rfl, ?_⟩
          simpa using tryExtract_sound cbor hc
        · simp at h
      · simp at h

/-- The bytes a Solidity constructor decodes as its arguments start at the compile-time program
size, which is fixed in the (matched) init code: `onchain.drop code.length`. After the fix they
are exactly the arguments verify reports. -/
theorem matchBytecodes_args {code args onchain : List UInt8} {hashNone : Bool} {t : MatchType}
    (hargs : args ≠ [])
    (h : matchBytecodes cbor (code ++ args) onchain args false hashNone = some t) :
    onchain.drop code.length = args := by
  obtain ⟨code', rfl, hl, _⟩ := matchBytecodes_creation_sound cbor hargs h
  rw [hl]; simp

/-- **Headline (verify, runtime code).** A runtime match is equality up to metadata. -/
theorem matchBytecodes_runtime_sound {loc onchain args : List UInt8} {hashNone : Bool}
    {t : MatchType} (h : matchBytecodes cbor loc onchain args true hashNone = some t) :
    SameUpToMetadata cbor loc onchain := by
  unfold matchBytecodes matchBytecodesWith isPartialMatchWith at h
  split at h
  · rename_i he; exact ⟨congrArg List.length he, Or.inl he⟩
  · simp only [Bool.or_true, if_true] at h
    split at h
    · rename_i hc; exact tryExtract_sound cbor hc
    · simp at h

/-! ## The bug before the fix

Before the fix both sides were stripped by their own trailing length. With ciborium accepting any
input that starts with one CBOR item, onchain code can declare a longer "metadata" region that
swallows bytes the constructor actually decodes as arguments. -/

/-- Local creation code `P ++ M ++ u16be(|M|)` with `P = [0x00]`, `M = [0x60]`. -/
def exCode : List UInt8 := [0x00, 0x60, 0x00, 0x01]

/-- Constructor arguments the user supplies. -/
def exArgs : List UInt8 := [0xaa]

/-- Onchain creation code `P ++ X ++ u16be(|X|) ++ args` with `X = [0x60, 0xbb]`. -/
def exOnchain : List UInt8 := [0x00, 0x60, 0xbb, 0x00, 0x02, 0xaa]

/-- **The bug.** For any CBOR predicate that, like ciborium, accepts `0x60` followed by
anything, the old `match_bytecodes` reports a partial match although the constructor decodes
`[0x02, 0xaa]`, not the supplied `[0xaa]`. The fixed version rejects it. -/
theorem old_accepts_wrong_args (hc : ∀ rest, cbor (0x60 :: rest) = true) :
    matchBytecodesOld cbor (exCode ++ exArgs) exOnchain exArgs false false = some .partialMatch ∧
      exOnchain.drop exCode.length ≠ exArgs ∧
      matchBytecodes cbor (exCode ++ exArgs) exOnchain exArgs false false = none := by
  have h1 := hc [0xbb]
  have h2 := hc []
  refine ⟨?_, by decide, ?_⟩ <;>
    simp [matchBytecodesWith, isPartialMatchWith, tryExtract, tryExtractOld, ignoreMetadataHash,
      findMetadataStart, trailingLen, u16be, exCode, exArgs, exOnchain, h1, h2, List.isSuffixOf]

end Deploy.Metadata
