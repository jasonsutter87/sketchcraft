//! Components inside components: how deep they may nest, that none may contain
//! itself, and how much geometry the placements add up to once everything is
//! expanded (a component placed 10 times, holding a component placed 10
//! times, and so on, is 10^N faces on screen from a file of a few hundred
//! bytes). The walk is iterative with its own stack, so a hostile or damaged
//! model can't overflow the program's.

use crate::model::{Id, Model, MAX_ENTITIES, MAX_GROWTH};
use std::collections::{HashMap, HashSet};

/// The most levels of components inside components (real models are about 10 deep).
pub const MAX_NEST: usize = 64;
/// The most geometry (edges, faces and placements) the placements may expand to: the person's, and an
/// agent's or a plugin's (who may not make a model bigger than this; what is already there is left alone).
pub const MAX_EXPANDED: usize = MAX_ENTITIES;
pub const MAX_EXPANDED_UNTRUSTED: usize = MAX_GROWTH;

/// What goes wrong with how components are placed in each other.
#[derive(Debug, Clone, PartialEq)]
pub enum NestProblem {
    /// a component that contains itself, directly or through others (its name)
    Loop(String),
    /// nested deeper than `MAX_NEST`
    TooDeep,
}

impl std::fmt::Display for NestProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NestProblem::Loop(name) => write!(f, "the component \"{name}\" contains itself (a component can't be placed inside itself, directly or through others)"),
            NestProblem::TooDeep => write!(f, "components are nested more than {MAX_NEST} levels deep"),
        }
    }
}

/// A component and what its copies add up to (see `Model::heaviest_components`).
#[derive(Debug, Clone, PartialEq)]
pub struct Heavy {
    pub name: String,
    /// how many times it is drawn, counting the copies of everything it sits in
    pub copies: usize,
    /// its own edges, faces and placements times `copies`
    pub total: usize,
}

/// A count for a sentence: 8,095,909 is "8.1 million", 12,400 is "12,400".
fn about(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1} million", n as f64 / 1e6)
    } else {
        thousands(n)
    }
}

fn thousands(n: usize) -> String {
    let d = n.to_string();
    let mut out = String::new();
    for (i, ch) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

struct Frame<'a> {
    node: *const Model,
    /// (what the placement names, how many copies it draws)
    kids: Vec<(&'a Model, usize, &'a str)>,
    next: usize,
    sum: usize,
}

/// A placement's copies: 1, unless its attributes ask for Dynamic Component copies (at most 1,001).
fn weight(i: &crate::model::Instance, d: &crate::model::Definition) -> usize {
    if i.attrs.is_empty() && d.attrs.is_empty() {
        return 1;
    }
    let has = |m: &std::collections::BTreeMap<String, String>| m.keys().any(|k| k.eq_ignore_ascii_case("copies"));
    if has(&i.attrs) || has(&d.attrs) {
        1001
    } else {
        1
    }
}

fn own(m: &Model) -> usize {
    // what drawing builds per placement: a face and an edge each cost an entry (points ride along with their faces)
    m.edges.len() + m.faces.len() + m.instances.len()
}

/// A model's placements, resolved the way drawing does: the root's table first, then the model's own.
fn kids<'a>(m: &'a Model, root: &'a Model) -> Vec<(&'a Model, usize, &'a str)> {
    m.instances
        .values()
        .filter_map(|i| {
            let d = root.definitions.get(&i.def).or_else(|| m.definitions.get(&i.def))?;
            Some((&*d.model, weight(i, d), d.name.as_str()))
        })
        .collect()
}

impl Model {
    /// How much geometry the model is once every placement is expanded (saturating), or the reason it can't be:
    /// a component that contains itself, or nesting past `MAX_NEST`. Each component is worked out once, however
    /// often it is placed. Cost: one pass over the placements.
    pub fn expanded_count(&self) -> Result<usize, NestProblem> {
        let mut memo: HashMap<*const Model, usize> = HashMap::new();
        let mut on_path: HashSet<*const Model> = HashSet::new();
        let top = self as *const Model;
        let mut stack = vec![Frame { node: top, kids: kids(self, self), next: 0, sum: own(self) }];
        on_path.insert(top);
        let mut result = 0usize;
        while !stack.is_empty() {
            let top = stack.len() - 1;
            if stack[top].next < stack[top].kids.len() {
                let (m, copies, name) = stack[top].kids[stack[top].next];
                stack[top].next += 1;
                let p = m as *const Model;
                if on_path.contains(&p) {
                    return Err(NestProblem::Loop(name.to_string()));
                }
                if let Some(&n) = memo.get(&p) {
                    stack[top].sum = stack[top].sum.saturating_add(n.saturating_mul(copies));
                    continue;
                }
                if stack.len() > MAX_NEST {
                    return Err(NestProblem::TooDeep);
                }
                on_path.insert(p);
                // a component's own (usually empty) table of definitions only resolves its own placements
                stack.push(Frame { node: p, kids: kids(m, self), next: 0, sum: own(m) });
            } else {
                let done = stack.pop().unwrap();
                on_path.remove(&done.node);
                memo.insert(done.node, done.sum);
                match stack.last_mut() {
                    Some(parent) => {
                        let copies = parent.kids[parent.next - 1].1;
                        parent.sum = parent.sum.saturating_add(done.sum.saturating_mul(copies));
                    }
                    None => result = done.sum,
                }
            }
        }
        Ok(result)
    }

    /// `expanded_count` as a sentence for the person, refusing a model that expands past `limit`.
    pub fn check_nesting(&self, limit: usize) -> Result<usize, String> {
        let n = self.expanded_count().map_err(|p| p.to_string())?;
        if n > limit {
            return Err(self.too_big_text(n, limit));
        }
        Ok(n)
    }

    /// Why a model that expands to `n` (past `limit`) is too big, naming the components that make it so and
    /// what to do about it (#296: a bare count gave the person nothing to act on).
    pub fn too_big_text(&self, n: usize, limit: usize) -> String {
        let mut s = format!("its components add up to about {} edges and faces once every copy is counted, and SketchCraft can draw up to {}", about(n), about(limit));
        let heavy = self.heaviest_components(3);
        if !heavy.is_empty() {
            let parts: Vec<String> = heavy
                .iter()
                .map(|h| format!("\"{}\" ({} {}, about {})", h.name, thousands(h.copies), if h.copies == 1 { "copy" } else { "copies" }, about(h.total)))
                .collect();
            s += &format!(". Most of it is {}", parts.join(", "));
        }
        s += ". To open it, make those components lighter in the program that made it, such as SketchUp (a simpler stand-in, or fewer copies), or save its parts as separate files and open them one at a time";
        s
    }

    /// The components that add the most geometry once every copy is drawn, heaviest first (at most `n`):
    /// a component's copies are its placements multiplied through every component it sits in. Gives nothing
    /// for a model with a loop (`expanded_count` reports that). One pass over the placements.
    pub fn heaviest_components(&self, n: usize) -> Vec<Heavy> {
        // every model reachable from the root, once each, in post-order (children before their parents)
        let root = self as *const Model;
        let mut order: Vec<*const Model> = Vec::new();
        let mut info: HashMap<*const Model, (Vec<(*const Model, usize)>, usize)> = HashMap::new();
        let mut names: HashMap<*const Model, &str> = HashMap::new();
        let mut done: HashMap<*const Model, bool> = HashMap::new(); // false: on the path, true: finished
        let edges = |m: &Model| kids(m, self).into_iter().map(|(k, c, _)| (k as *const Model, c)).collect::<Vec<_>>();
        info.insert(root, (edges(self), own(self)));
        done.insert(root, false);
        let mut stack: Vec<(*const Model, &Model, usize)> = vec![(root, self, 0)];
        while let Some(&(p, m, next)) = stack.last() {
            let ks = kids(m, self);
            if next < ks.len() {
                stack.last_mut().unwrap().2 += 1;
                let (km, _, name) = ks[next];
                let kp = km as *const Model;
                names.entry(kp).or_insert(name);
                match done.get(&kp) {
                    Some(false) => return Vec::new(),
                    Some(true) => continue,
                    None => {}
                }
                if stack.len() > MAX_NEST {
                    return Vec::new();
                }
                done.insert(kp, false);
                info.insert(kp, (edges(km), own(km)));
                stack.push((kp, km, 0));
            } else {
                stack.pop();
                done.insert(p, true);
                order.push(p);
            }
        }
        // reverse post-order: every parent before its children, so its copies are final when they are handed down
        let mut copies: HashMap<*const Model, usize> = HashMap::new();
        copies.insert(root, 1);
        for &p in order.iter().rev() {
            let c = copies.get(&p).copied().unwrap_or(0);
            for &(k, w) in &info[&p].0 {
                let e = copies.entry(k).or_insert(0);
                *e = e.saturating_add(c.saturating_mul(w));
            }
        }
        let mut out: Vec<Heavy> = order
            .iter()
            .filter(|&&p| p != root)
            .map(|p| {
                let c = copies.get(p).copied().unwrap_or(0);
                Heavy { name: names.get(p).copied().unwrap_or("").to_string(), copies: c, total: c.saturating_mul(info[p].1) }
            })
            .filter(|h| h.total > 0)
            .collect();
        out.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
        out.truncate(n);
        out
    }

    /// The ids of every component that `def` uses, at any depth (itself not included unless it is in a loop).
    pub fn uses_of(&self, def: Id) -> std::collections::BTreeSet<Id> {
        let mut out = std::collections::BTreeSet::new();
        let mut todo = vec![def];
        while let Some(d) = todo.pop() {
            if let Some(d) = self.definitions.get(&d) {
                for i in d.model.instances.values() {
                    if out.insert(i.def) {
                        todo.push(i.def);
                    }
                }
            }
        }
        out
    }
}
