/-!
# `expectCall` counting

Model of `expect_call`, `observe_call` and `first_unmet_call` in
`crates/cheatcodes/src/test/expect.rs`, for one callee.

Fidelity:
- `ExpectedCallsForTarget` (a `HashMap` keyed by calldata and call scheme) is an association
  list with unique keys; its iteration order does not matter because every entry is updated.
- Calldata is a `List Nat` of bytes, the call scheme is a `Nat`, `msg.value` an `Option Nat`
  (`None` for calls that transfer no value) and gas a `Nat`.
- The `2300` gas stipend that `expect_call` adds for a positive value is applied by the caller
  of `register` and not modelled; it saturates at `u64::MAX`.

`observe_counts` states that each expectation counts exactly the observed calls that pass all of
its filters. `repeated_filters_dropped` records the keyed-additive semantics of repeated
non-counted expectations, which both the concrete and the symbolic engine implement on purpose:
the filters of a repeat with the same calldata and scheme are dropped, so a test expecting one
call with value 1 and one with value 2 passes on two calls with value 1.
-/

namespace Cheatcodes.ExpectCall

/-- A call made to the callee. -/
structure Call where
  input : List Nat
  value : Option Nat
  gas : Nat
  scheme : Nat

/-- `ExpectedCallData` with its key. -/
structure Exp where
  calldata : List Nat
  scheme : Option Nat
  value : Option Nat
  gas : Option Nat
  minGas : Option Nat
  count : Nat
  exact : Bool

/-- The filter of `observe_call`. -/
def Matches (e : Exp) (c : Call) : Prop :=
  e.calldata <+: c.input ∧ e.value.all (some · == c.value) = true ∧ e.gas.all (· == c.gas) = true ∧
    e.minGas.all (· ≤ c.gas) = true ∧ e.scheme.all (· == c.scheme) = true

instance (e : Exp) (c : Call) : Decidable (Matches e c) := by unfold Matches; infer_instance

/-- Models `expect_call`: `none` is an error. A repeat of a non-counted key increments the count of
the existing entry and keeps the existing filters. -/
def register (t : List (Exp × Nat)) (e : Exp) : Option (List (Exp × Nat)) :=
  match t.find? (fun p => p.1.calldata == e.calldata && p.1.scheme == e.scheme) with
  | none => some (t ++ [(e, 0)])
  | some (old, _) =>
    if e.exact || old.exact then none
    else some (t.map fun p =>
      if p.1.calldata == e.calldata && p.1.scheme == e.scheme then
        ({ p.1 with count := p.1.count + 1 }, p.2)
      else p)

/-- Models `observe_call`. -/
def observe (t : List (Exp × Nat)) (c : Call) : List (Exp × Nat) :=
  t.map fun p => (p.1, if Matches p.1 c then p.2 + 1 else p.2)

/-- Models `first_unmet_call` returning `None`. -/
def passes (t : List (Exp × Nat)) : Bool :=
  t.all fun p => if p.1.exact then p.2 == p.1.count else decide (p.1.count ≤ p.2)

theorem observe_counts (t : List (Exp × Nat)) (calls : List Call) :
    calls.foldl observe t =
      t.map fun p => (p.1, p.2 + (calls.filter fun c => decide (Matches p.1 c)).length) := by
  induction calls generalizing t with
  | nil => simp
  | cons c cs ih =>
    rw [List.foldl_cons, ih]
    unfold observe
    simp only [List.map_map]
    congr 1
    funext p
    by_cases h : Matches p.1 c <;> simp [h] <;> omega

/-- The keyed-additive semantics: the value filter of the second expectation is dropped. -/
theorem repeated_filters_dropped :
    let e1 : Exp := ⟨[1], none, some 1, none, none, 1, false⟩
    let e2 : Exp := { e1 with value := some 2 }
    let call : Call := ⟨[1], some 1, 0, 0⟩
    ∃ t, ((register [] e1).bind fun t => register t e2) = some t ∧
      passes ([call, call].foldl observe t) = true := by
  refine ⟨_, rfl, ?_⟩
  decide

end Cheatcodes.ExpectCall
