// Differential fuzzer for the symbolic rewrites; see `README.md` in this directory.
use super::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn word(&mut self) -> U256 {
        let pool = [
            U256::ZERO,
            U256::ONE,
            U256::from(2),
            U256::from(3),
            U256::from(7),
            U256::from(8),
            U256::from(31),
            U256::from(32),
            U256::from(255),
            U256::from(256),
            U256::from(0x7f),
            U256::from(0x80),
            U256::ONE << 128,
            U256::ONE << 255,
            U256::MAX,
            U256::MAX - U256::ONE,
            (U256::ONE << 160) - U256::ONE,
            U256::from(10).pow(U256::from(18)),
            (U256::ONE << 255) - U256::ONE,
            U256::from(1000),
        ];
        match self.below(4) {
            0 => U256::from_limbs([self.next(), self.next(), self.next(), self.next()]),
            1 => U256::from(self.below(20)),
            _ => pool[self.below(pool.len() as u64) as usize],
        }
    }
}

#[derive(Clone, Debug)]
enum R {
    C(U256),
    V(usize),
    Not(Box<R>),
    Bin(SymBinOp, Box<R>, Box<R>),
    Tern(SymTernOp, Box<R>, Box<R>, Box<R>),
    Ite(Box<RB>, Box<R>, Box<R>),
    SignExt(Box<R>, Box<R>),
    Byte(Box<R>, Box<R>),
    Clz(Box<R>),
}

#[derive(Clone, Debug)]
enum RB {
    Cmp(SymCmpOp, R, R),
    Not(Box<RB>),
    And(Box<RB>, Box<RB>),
    Or(Box<RB>, Box<RB>),
}

fn reval(r: &R, env: &[U256]) -> U256 {
    match r {
        R::C(v) => *v,
        R::V(i) => env[*i],
        R::Not(a) => !reval(a, env),
        R::Bin(op, a, b) => op.eval(reval(a, env), reval(b, env)),
        R::Tern(op, a, b, m) => op.eval(reval(a, env), reval(b, env), reval(m, env)),
        R::Ite(c, a, b) => {
            if rbeval(c, env) {
                reval(a, env)
            } else {
                reval(b, env)
            }
        }
        R::SignExt(i, v) => signextend(reval(i, env), reval(v, env)),
        R::Byte(i, v) => {
            let i = reval(i, env);
            if i >= U256::from(32) {
                U256::ZERO
            } else {
                U256::from(reval(v, env).to_be_bytes::<32>()[i.to::<usize>()])
            }
        }
        R::Clz(v) => U256::from(reval(v, env).leading_zeros()),
    }
}

fn rbeval(r: &RB, env: &[U256]) -> bool {
    match r {
        RB::Cmp(op, a, b) => op.eval(reval(a, env), reval(b, env)),
        RB::Not(a) => !rbeval(a, env),
        RB::And(a, b) => rbeval(a, env) && rbeval(b, env),
        RB::Or(a, b) => rbeval(a, env) || rbeval(b, env),
    }
}

const OPS: [SymBinOp; 14] = [
    SymBinOp::Add,
    SymBinOp::Sub,
    SymBinOp::Mul,
    SymBinOp::UDiv,
    SymBinOp::URem,
    SymBinOp::SDiv,
    SymBinOp::SRem,
    SymBinOp::And,
    SymBinOp::Or,
    SymBinOp::Xor,
    SymBinOp::Shl,
    SymBinOp::Shr,
    SymBinOp::Sar,
    SymBinOp::Mul,
];

const CMPS: [SymCmpOp; 7] = [
    SymCmpOp::Eq,
    SymCmpOp::Ult,
    SymCmpOp::Ugt,
    SymCmpOp::Ule,
    SymCmpOp::Uge,
    SymCmpOp::Slt,
    SymCmpOp::Sgt,
];

struct Gen<'a> {
    rng: Rng,
    cx: &'a mut SymCx,
    vars: Vec<SymExpr>,
    // Reusable subterms to create sharing, which most rewrites need.
    pool: Vec<(SymExpr, R)>,
}

impl Gen<'_> {
    fn word(&mut self, depth: u32) -> (SymExpr, R) {
        if !self.pool.is_empty() && self.rng.below(4) == 0 {
            let i = self.rng.below(self.pool.len() as u64) as usize;
            return self.pool[i].clone();
        }
        let leaf = depth == 0 || self.rng.below(4) == 0;
        let out = if leaf {
            if self.rng.below(2) == 0 {
                let i = self.rng.below(self.vars.len() as u64) as usize;
                (self.vars[i].clone(), R::V(i))
            } else {
                let v = self.rng.word();
                (SymExpr::constant(self.cx, v), R::C(v))
            }
        } else {
            match self.rng.below(16) {
                7 | 8 | 9 | 10 => self.template(depth - 1),
                0 => {
                    let (a, ra) = self.word(depth - 1);
                    (SymExpr::not(self.cx, a), R::Not(Box::new(ra)))
                }
                1 => {
                    let op = [SymTernOp::AddMod, SymTernOp::MulMod, SymTernOp::MulDiv]
                        [self.rng.below(3) as usize];
                    let (a, ra) = self.word(depth - 1);
                    let (b, rb) = self.word(depth - 1);
                    let (m, rm) = self.word(depth - 1);
                    (
                        SymExpr::ternop(self.cx, op, a, b, m),
                        R::Tern(op, Box::new(ra), Box::new(rb), Box::new(rm)),
                    )
                }
                2 => {
                    let (c, rc) = self.boolean(depth - 1);
                    let (a, ra) = self.word(depth - 1);
                    let (b, rb) = self.word(depth - 1);
                    (
                        SymExpr::ite(self.cx, c, a, b),
                        R::Ite(Box::new(rc), Box::new(ra), Box::new(rb)),
                    )
                }
                3 => {
                    let (c, rc) = self.boolean(depth - 1);
                    (
                        SymExpr::bool_word(self.cx, c),
                        R::Ite(Box::new(rc), Box::new(R::C(U256::ONE)), Box::new(R::C(U256::ZERO))),
                    )
                }
                4 if self.rng.below(4) == 0 => {
                    let (i, ri) = self.word(depth - 1);
                    let (v, rv) = self.word(depth - 1);
                    (signextend_word_dynamic(self.cx, i, v), R::SignExt(Box::new(ri), Box::new(rv)))
                }
                5 if self.rng.below(4) == 0 => {
                    let (i, ri) = self.word(depth - 1);
                    let (v, rv) = self.word(depth - 1);
                    (byte_word_dynamic(self.cx, i, v), R::Byte(Box::new(ri), Box::new(rv)))
                }
                6 if self.rng.below(4) == 0 => {
                    let (v, rv) = self.word(depth - 1);
                    (clz_word(self.cx, v), R::Clz(Box::new(rv)))
                }
                _ => {
                    let op = OPS[self.rng.below(OPS.len() as u64) as usize];
                    let (a, ra) = self.word(depth - 1);
                    let (b, rb) = if self.rng.below(3) == 0 {
                        let v = self.rng.word();
                        (SymExpr::constant(self.cx, v), R::C(v))
                    } else {
                        self.word(depth - 1)
                    };
                    let e = if matches!(
                        op,
                        SymBinOp::UDiv | SymBinOp::URem | SymBinOp::SDiv | SymBinOp::SRem
                    ) {
                        let zero = SymExpr::zero(self.cx);
                        let cond = SymBoolExpr::eq(self.cx, b.clone(), zero.clone());
                        let raw = SymExpr::binop(self.cx, op, a, b);
                        SymExpr::ite(self.cx, cond, zero, raw)
                    } else {
                        SymExpr::binop(self.cx, op, a, b)
                    };
                    (e, R::Bin(op, Box::new(ra), Box::new(rb)))
                }
            }
        };
        if self.rng.below(3) == 0 && self.pool.len() < 16 {
            self.pool.push(out.clone());
        }
        out
    }

    fn op2(&mut self, op: SymBinOp, a: (SymExpr, R), b: (SymExpr, R)) -> (SymExpr, R) {
        let (a, ra) = a;
        let (b, rb) = b;
        let e = if matches!(op, SymBinOp::UDiv | SymBinOp::URem | SymBinOp::SDiv | SymBinOp::SRem) {
            let zero = SymExpr::zero(self.cx);
            let cond = SymBoolExpr::eq(self.cx, b.clone(), zero.clone());
            let raw = SymExpr::binop(self.cx, op, a, b);
            SymExpr::ite(self.cx, cond, zero, raw)
        } else {
            SymExpr::binop(self.cx, op, a, b)
        };
        (e, R::Bin(op, Box::new(ra), Box::new(rb)))
    }

    fn cnst(&mut self, v: U256) -> (SymExpr, R) {
        (SymExpr::constant(self.cx, v), R::C(v))
    }

    fn small_const(&mut self) -> (SymExpr, R) {
        let v = match self.rng.below(4) {
            0 => U256::ONE << self.rng.below(256),
            1 => U256::from(self.rng.below(50) + 1),
            2 => (U256::ONE << self.rng.below(256)) - U256::ONE,
            _ => self.rng.word(),
        };
        self.cnst(v)
    }

    fn template(&mut self, depth: u32) -> (SymExpr, R) {
        use SymBinOp::*;
        let a = self.word(depth);
        let d = if self.rng.below(2) == 0 { self.small_const() } else { self.word(depth) };
        match self.rng.below(14) {
            0 => {
                let q = self.op2(UDiv, a.clone(), d.clone());
                let p = self.op2(Mul, q, d.clone());
                let r = self.op2(URem, a, d);
                self.op2(Add, p, r)
            }
            1 => {
                let r = self.op2(URem, a.clone(), d);
                self.op2(Sub, a, r)
            }
            2 => {
                let q = self.op2(UDiv, a, d.clone());
                self.op2(Mul, q, d)
            }
            3 => {
                let b = self.word(depth);
                let p = self.op2(Mul, a.clone(), b);
                self.op2(UDiv, p, a)
            }
            4 => {
                let b = self.word(depth);
                let c = self.small_const();
                let p = self.op2(Mul, a, b);
                let s = self.op2(Add, p, c);
                self.op2(UDiv, s, d)
            }
            5 => {
                let n = self.rng.below(256) as usize;
                let m = self.cnst((U256::ONE << n) - U256::ONE);
                let low = self.op2(And, a.clone(), m);
                let diff = self.op2(Sub, a, low);
                let p = self.cnst(U256::ONE << n);
                self.op2(UDiv, diff, p)
            }
            6 => {
                let n = U256::from(self.rng.below(260));
                let n = self.cnst(n);
                let l = self.op2(Shl, a, n.clone());
                self.op2(Shr, l, n)
            }
            7 => {
                let one = self.cnst(U256::ONE);
                let s = self.op2(Add, a, d.clone());
                let s = self.op2(Sub, s, one);
                let q = self.op2(UDiv, s, d.clone());
                if self.rng.below(2) == 0 { q } else { self.op2(Mul, q, d) }
            }
            8 => {
                let b = self.word(depth);
                let max = self.cnst(U256::MAX);
                let mm =
                    SymExpr::ternop(self.cx, SymTernOp::MulMod, a.0.clone(), b.0.clone(), max.0);
                let rmm = R::Tern(
                    SymTernOp::MulMod,
                    Box::new(a.1.clone()),
                    Box::new(b.1.clone()),
                    Box::new(max.1),
                );
                let lo = self.op2(Mul, a, b);
                let diff = self.op2(Sub, (mm.clone(), rmm.clone()), lo.clone());
                let lt = SymBoolExpr::cmp(self.cx, SymCmpOp::Ult, mm, lo.0.clone());
                let ltw = SymExpr::bool_word(self.cx, lt);
                let rlt = R::Ite(
                    Box::new(RB::Cmp(SymCmpOp::Ult, rmm, lo.1)),
                    Box::new(R::C(U256::ONE)),
                    Box::new(R::C(U256::ZERO)),
                );
                self.op2(Sub, diff, (ltw, rlt))
            }
            9 => {
                let n = self.rng.below(256) as usize;
                let b = self.word(depth);
                let m = self.cnst(U256::ONE << n);
                let op = if self.rng.below(2) == 0 { SymTernOp::AddMod } else { SymTernOp::MulMod };
                (
                    SymExpr::ternop(self.cx, op, a.0, b.0, m.0),
                    R::Tern(op, Box::new(a.1), Box::new(b.1), Box::new(m.1)),
                )
            }
            10 => {
                let k = U256::from(8 * self.rng.below(33));
                let k = self.cnst(k);
                self.op2(Shr, a, k)
            }
            11 => {
                let mask = self.small_const();
                let b = self.word(depth);
                let o = self.op2(Or, a, b);
                self.op2(And, o, mask)
            }
            12 => {
                let q = self.op2(SDiv, a.clone(), d.clone());
                let r = self.op2(SRem, a, d.clone());
                let p = self.op2(Mul, q, d);
                self.op2(Add, p, r)
            }
            _ => {
                let q = self.op2(UDiv, a.clone(), d);
                self.op2(Sub, a, q)
            }
        }
    }

    fn bool_template(&mut self, depth: u32) -> (SymBoolExpr, RB) {
        use SymBinOp::*;
        let x = self.word(depth);
        let y = self.word(depth);
        match self.rng.below(8) {
            0 => {
                let p = self.op2(Mul, x.clone(), y.clone());
                let q = self.op2(UDiv, p, x.clone());
                let zero = SymExpr::zero(self.cx);
                let c1 = SymBoolExpr::eq(self.cx, x.0.clone(), zero);
                let c2 = SymBoolExpr::eq(self.cx, q.0.clone(), y.0.clone());
                (
                    SymBoolExpr::or(self.cx, vec![c1, c2]),
                    RB::Or(
                        Box::new(RB::Cmp(SymCmpOp::Eq, x.1, R::C(U256::ZERO))),
                        Box::new(RB::Cmp(SymCmpOp::Eq, q.1, y.1)),
                    ),
                )
            }
            1 => {
                let c = self.small_const();
                let q = self.op2(UDiv, c, x.clone());
                let op = if self.rng.below(2) == 0 { SymCmpOp::Ule } else { SymCmpOp::Ult };
                let (l, r) = if op == SymCmpOp::Ule { (x, q) } else { (q, x) };
                (SymBoolExpr::cmp(self.cx, op, l.0, r.0), RB::Cmp(op, l.1, r.1))
            }
            2 => {
                let d = self.small_const();
                let q = self.op2(UDiv, x, d);
                let k = self.small_const();
                let op = CMPS[self.rng.below(CMPS.len() as u64) as usize];
                if self.rng.below(2) == 0 {
                    (SymBoolExpr::cmp(self.cx, op, q.0, k.0), RB::Cmp(op, q.1, k.1))
                } else {
                    (SymBoolExpr::cmp(self.cx, op, k.0, q.0), RB::Cmp(op, k.1, q.1))
                }
            }
            3 => {
                let s = self.op2(Add, x.clone(), y);
                (
                    SymBoolExpr::cmp(self.cx, SymCmpOp::Ult, s.0, x.0),
                    RB::Cmp(SymCmpOp::Ult, s.1, x.1),
                )
            }
            4 => {
                let q = self.op2(UDiv, x, y);
                let zero = SymExpr::zero(self.cx);
                let e = SymBoolExpr::eq(self.cx, q.0, zero);
                let r = RB::Cmp(SymCmpOp::Eq, q.1, R::C(U256::ZERO));
                if self.rng.below(2) == 0 { (e, r) } else { (e.not(self.cx), RB::Not(Box::new(r))) }
            }
            5 => {
                let k = self.small_const();
                let op = [SymCmpOp::Ult, SymCmpOp::Ule, SymCmpOp::Eq, SymCmpOp::Ugt]
                    [self.rng.below(4) as usize];
                (SymBoolExpr::cmp(self.cx, op, x.0, k.0), RB::Cmp(op, x.1, k.1))
            }
            6 => {
                let zero = SymExpr::zero(self.cx);
                let e = SymBoolExpr::eq(self.cx, x.0, zero);
                (e.not(self.cx), RB::Not(Box::new(RB::Cmp(SymCmpOp::Eq, x.1, R::C(U256::ZERO)))))
            }
            _ => {
                let p = self.op2(Mul, x.clone(), y.clone());
                let hi = (
                    SymExpr::binop(self.cx, SymBinOp::MulHigh, x.0.clone(), y.0.clone()),
                    R::Bin(SymBinOp::MulHigh, Box::new(x.1.clone()), Box::new(y.1.clone())),
                );
                let _ = p;
                let op = CMPS[self.rng.below(CMPS.len() as u64) as usize];
                (SymBoolExpr::cmp(self.cx, op, hi.0, y.0), RB::Cmp(op, hi.1, y.1))
            }
        }
    }

    fn boolean(&mut self, depth: u32) -> (SymBoolExpr, RB) {
        if self.rng.below(3) == 0 {
            return self.bool_template(depth.saturating_sub(1));
        }
        match if depth == 0 { 0 } else { self.rng.below(5) } {
            0 | 1 | 2 => {
                let op = CMPS[self.rng.below(CMPS.len() as u64) as usize];
                let (a, ra) = self.word(depth.saturating_sub(1));
                let (b, rb) = if self.rng.below(3) == 0 {
                    let v = self.rng.word();
                    (SymExpr::constant(self.cx, v), R::C(v))
                } else {
                    self.word(depth.saturating_sub(1))
                };
                (SymBoolExpr::cmp(self.cx, op, a, b), RB::Cmp(op, ra, rb))
            }
            3 => {
                let (a, ra) = self.boolean(depth - 1);
                let (b, rb) = self.boolean(depth - 1);
                if self.rng.below(2) == 0 {
                    (SymBoolExpr::and(self.cx, vec![a, b]), RB::And(Box::new(ra), Box::new(rb)))
                } else {
                    (SymBoolExpr::or(self.cx, vec![a, b]), RB::Or(Box::new(ra), Box::new(rb)))
                }
            }
            _ => {
                let (a, ra) = self.boolean(depth - 1);
                (a.not(self.cx), RB::Not(Box::new(ra)))
            }
        }
    }
}

/// SMT-LIB meaning of a word.
fn smt_eval(e: &SymExpr, m: &SymbolicModel, memo: &mut HashMap<SymExpr, U256>) -> U256 {
    if let Some(v) = memo.get(e) {
        return *v;
    }
    let v = match e.kind() {
        SymExprKind::Const(v) => *v,
        SymExprKind::Var(s) => m.get(s).copied().unwrap_or_default(),
        SymExprKind::Not(a) => !smt_eval(a, m, memo),
        SymExprKind::BinOp(op, a, b) => {
            let a = smt_eval(a, m, memo);
            let b = smt_eval(b, m, memo);
            if b.is_zero() {
                match op {
                    SymBinOp::UDiv => U256::MAX,
                    SymBinOp::URem | SymBinOp::SRem => a,
                    SymBinOp::SDiv => {
                        if a.bit(255) {
                            U256::ONE
                        } else {
                            U256::MAX
                        }
                    }
                    _ => op.eval(a, b),
                }
            } else {
                op.eval(a, b)
            }
        }
        SymExprKind::TernOp(op, a, b, c) => {
            op.eval(smt_eval(a, m, memo), smt_eval(b, m, memo), smt_eval(c, m, memo))
        }
        SymExprKind::Ite(c, a, b) => {
            if smt_eval_bool(c, m, memo) {
                smt_eval(a, m, memo)
            } else {
                smt_eval(b, m, memo)
            }
        }
        _ => panic!("unexpected leaf"),
    };
    memo.insert(e.clone(), v);
    v
}

fn smt_eval_bool(e: &SymBoolExpr, m: &SymbolicModel, memo: &mut HashMap<SymExpr, U256>) -> bool {
    match e.kind() {
        SymBoolExprKind::Const(b) => *b,
        SymBoolExprKind::Not(a) => !smt_eval_bool(a, m, memo),
        SymBoolExprKind::And(xs) => xs.iter().all(|x| smt_eval_bool(x, m, memo)),
        SymBoolExprKind::Cmp(op, a, b) => op.eval(smt_eval(a, m, memo), smt_eval(b, m, memo)),
    }
}

fn run(seed: u64, iters: usize, depth: u32, nvars: usize, nconstraints: usize) -> usize {
    let mut failures = 0;
    let mut changed = 0usize;
    let mut raw_div = 0usize;
    let mut total = 0usize;
    for it in 0..iters {
        let mut cx = SymCx::new();
        let vars: Vec<_> = (0..nvars).map(|i| SymExpr::var(&mut cx, &format!("x{i}"))).collect();
        let syms: Vec<_> = vars.iter().map(|v| v.kind().get_var().unwrap()).collect();
        let mut g = Gen {
            rng: Rng(seed.wrapping_add((it as u64).wrapping_mul(0x9E3779B97F4A7C15)) | 1),
            cx: &mut cx,
            vars,
            pool: vec![],
        };
        // Construction check.
        let mut envs = vec![];
        for _ in 0..4 {
            envs.push((0..nvars).map(|_| g.rng.word()).collect::<Vec<_>>());
        }
        let (w, rw) = g.word(depth);
        for env in &envs {
            let model: SymbolicModel = syms.iter().copied().zip(env.iter().copied()).collect();
            let got = w.eval_model(&model).unwrap();
            let want = reval(&rw, env);
            if got != want {
                failures += 1;
                if failures < 5 {
                    eprintln!(
                        "CONSTRUCTION MISMATCH seed={seed} it={it} env={env:?}\n ref={rw:?}\n got={got} want={want}\n expr={w:?}"
                    );
                }
            }
        }
        // Normalization check: constraints true at env0.
        let env = envs[0].clone();
        let model: SymbolicModel = syms.iter().copied().zip(env.iter().copied()).collect();
        let mut constraints = vec![];
        let mut rcons = vec![];
        for _ in 0..nconstraints {
            let (c, rc) = g.boolean(depth);
            let (c, rc) =
                if rbeval(&rc, &env) { (c, rc) } else { (c.not(g.cx), RB::Not(Box::new(rc))) };
            // Construction consistency for Booleans.
            let got = c.eval_model(&model).unwrap();
            if !got {
                failures += 1;
                if failures < 5 {
                    eprintln!(
                        "BOOL CONSTRUCTION MISMATCH seed={seed} it={it} env={env:?}\n ref={rc:?}\n expr={c:?}"
                    );
                }
                continue;
            }
            constraints.push(c);
            rcons.push(rc);
        }
        let cx = g.cx;
        let mut cache = NormalizationCache::default();
        let normalized = normalize_sat_constraints(cx, &constraints, &mut cache);
        total += 1;
        if normalized != constraints {
            changed += 1;
        }
        if normalized.iter().any(|c| c.visit_bool(|e| matches!(e.kind(), SymExprKind::BinOp(SymBinOp::UDiv | SymBinOp::URem, _, d) if d.as_const().is_none()) && !matches!(e.kind(), SymExprKind::Const(_)))) {
            raw_div += 1;
        }
        let mut memo = HashMap::default();
        for n in &normalized {
            if !smt_eval_bool(n, &model, &mut memo) {
                failures += 1;
                if failures < 8 {
                    eprintln!(
                        "NORMALIZATION DROPPED A TRUE POINT seed={seed} it={it} env={env:?}\n refs={rcons:?}\n original={constraints:?}\n normalized_false={n:?}"
                    );
                }
                break;
            }
        }
        if constraints_are_directly_unsat(cx, &normalized) {
            failures += 1;
            if failures < 8 {
                eprintln!("DIRECT UNSAT ON SAT seed={seed} it={it} env={env:?} refs={rcons:?}");
            }
        }
        if product_monotonic_unsat_normalized(&normalized) {
            failures += 1;
            if failures < 8 {
                eprintln!("MONOTONIC UNSAT ON SAT seed={seed} it={it} env={env:?} refs={rcons:?}");
            }
        }
        if let Some(CheckedXorResult::Unsat) = checked_xor_result(&constraints) {
            failures += 1;
            if failures < 8 {
                eprintln!("XOR UNSAT ON SAT seed={seed} it={it} env={env:?} refs={rcons:?}");
            }
        }
    }
    eprintln!("stats: total={total} changed={changed} raw_symbolic_div={raw_div}");
    failures
}

#[test]
fn fuzz_tmp() {
    let seed: u64 = std::env::var("FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let iters: usize =
        std::env::var("FUZZ_ITERS").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let depth: u32 = std::env::var("FUZZ_DEPTH").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let nvars: usize = std::env::var("FUZZ_VARS").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
    let ncons: usize = std::env::var("FUZZ_CONS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let failures = run(seed, iters, depth, nvars, ncons);
    assert_eq!(failures, 0);
}
