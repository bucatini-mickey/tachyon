//! Plain-text description of a cell: everything modelled there.

use crate::geom::Pos;
use crate::scene::Scene;
use crate::things::Layer;
use crate::tile::Tile;
use crate::world::{FULL, SKY_MAX};

pub fn describe(scene: &Scene, p: Pos) -> Vec<String> {
    let w = &scene.world;
    let mut out = Vec::new();
    let Some(i) = w.index(p) else {
        out.push("outside the world".into());
        return out;
    };
    out.push(format!("({}, {})  level {}", p.x, p.y, p.z));
    let tile = w.tile[i];
    let m = w.mats.get(w.mat[i]);
    out.push(match tile {
        Tile::Open => "open space".to_string(),
        t => format!("{} of {}", t.name(), m.name),
    });
    if tile == Tile::Open && p.z > 0 {
        let b = w.idx(p.x, p.y, p.z - 1);
        if w.tile[b].is_wall() {
            out.push(format!("  on top of {}", w.mats.get(w.mat[b]).name));
        }
    }
    if w.fire[i] > 0 {
        out.push(format!("BURNING ({:.0}s left)", w.fire[i] as f32 / 10.0));
    }
    out.push(format!("temperature {:.1} C", w.temp[i]));
    if w.fluid[i] > 0 {
        out.push(format!("{} {}/{}", w.mats.get(w.fluid_mat[i]).name, w.fluid[i], FULL));
    }
    if w.gas[i] > 0 {
        out.push(format!("{} {}/255", w.mats.get(w.gas_mat[i]).name, w.gas[i]));
    }
    out.push(format!("sky light {}/{}", w.sky[i], SKY_MAX));
    if let Some(s) = scene.life.stains.get(p) {
        let age = if s.age < 3600.0 { "fresh" } else { "dried" };
        out.push(format!("{age} blood ({:.0} ml)", s.ml));
    }

    let things = &scene.things;
    let mut list: Vec<_> = things.at(p).collect();
    list.sort_by_key(|(_, t)| std::cmp::Reverse(things.kind_of(t).layer));
    let has_surface = list.iter().any(|(_, t)| things.kind_of(t).surface);
    for (id, t) in list {
        let k = things.kind_of(t);
        let mut s = match &t.name {
            Some(n) => format!("{} ({})", n, k.name),
            None => k.name.clone(),
        };
        if let Some(c) = scene.life.get(id) {
            if !c.alive {
                s.push_str(" - dead");
            } else if c.out {
                s.push_str(" - unconscious");
            } else if c.mind.asleep {
                s.push_str(" - asleep");
            } else if c.caps.blood < 0.9 || c.caps.pain > 1.0 {
                s.push_str(" - hurt");
            }
        }
        if k.door {
            s.push_str(if t.open { " - open" } else { " - shut" });
        }
        if k.layer == Layer::Item && has_surface {
            s = format!("  on it: {s}");
        }
        out.push(s);
    }
    out
}
