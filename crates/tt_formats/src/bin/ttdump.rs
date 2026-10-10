//! Command-line inspector for the game archive.
//!
//!   ttdump ARCHIVE list [SUBSTRING]
//!   ttdump ARCHIVE cat PATH
//!   ttdump ARCHIVE extract OUT_DIR SUBSTRING...
//!   ttdump ARCHIVE ghg PATH
//!   ttdump ARCHIVE an3 PATH

use std::path::PathBuf;
use tt_formats::dat::Archive;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> tt_formats::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: ttdump ARCHIVE (list [SUBSTRING] | cat PATH | extract OUT_DIR SUBSTRING... | ghg PATH | an3 PATH)");
        std::process::exit(2);
    }
    let archive = Archive::open(&args[0])?;
    let matches = |path: &str, needle: &str| path.to_ascii_lowercase().contains(&needle.replace('/', "\\").to_ascii_lowercase());
    match args[1].as_str() {
        "list" => {
            let needle = args.get(2).cloned().unwrap_or_default();
            let mut count = 0;
            for path in archive.paths().filter(|p| matches(p, &needle)) {
                let entry = archive.entry(path).unwrap();
                println!("{:>10} {} {path}", entry.size, entry.mode);
                count += 1;
            }
            eprintln!("{count} of {} files", archive.len());
        }
        "cat" => print!("{}", archive.read_text(&args[2])?),
        "extract" => {
            let out = PathBuf::from(&args[2]);
            let mut count = 0;
            for path in archive.paths().filter(|p| args[3..].iter().any(|n| matches(p, n))) {
                let target = out.join(path.replace('\\', "/"));
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::write(&target, archive.read(path)?)?;
                count += 1;
            }
            eprintln!("extracted {count} files to {}", out.display());
        }
        "ghg" => tt_formats::ghg::Model::parse(&archive.read(&args[2])?)?.print_summary(),
        "an3" => tt_formats::an3::Animation::parse(&archive.read(&args[2])?)?.print_summary(),
        "rtl" => {
            for (i, l) in tt_formats::rtl::parse(&archive.read(&args[2])?)?.iter().enumerate() {
                println!(
                    "{i:3} kind {} pos ({:6.2},{:6.2},{:6.2}) dir ({:5.2},{:5.2},{:5.2}) colour ({:.2},{:.2},{:.2}) radius {:.1}..{:.1} intensity {:.2} pitch {} yaw {} masks {:#x}/{:#x} chain {}{}",
                    l.kind, l.position[0], l.position[1], l.position[2], l.direction[0], l.direction[1], l.direction[2],
                    l.colour[0], l.colour[1], l.colour[2], l.inner_radius, l.outer_radius, l.intensity, l.pitch, l.yaw,
                    l.include_mask, l.exclude_mask, l.chain, if l.disabled { " disabled" } else { "" }
                );
            }
        }
        "ter" => {
            let t = tt_formats::ter::Terrain::parse(&archive.read(&args[2])?)?;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            let (mut floors, mut walls) = (0, 0);
            let mut surfaces = std::collections::BTreeMap::new();
            for tri in &t.triangles {
                if tri.normal[1] > 0.707 { floors += 1 } else { walls += 1 }
                *surfaces.entry(tri.surface).or_insert(0) += 1;
                for p in tri.points {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
            let fixed = t.groups.iter().filter(|g| g.kind == 0).count();
            println!("version {}, {} groups ({fixed} fixed), {} triangles ({floors} walkable, {walls} steep)", t.version, t.groups.len(), t.triangles.len());
            println!("bounds {lo:?} .. {hi:?}");
            println!("surface types {surfaces:?}");
        }
        // check CHAR_DIR MODEL_FILE [ANIM FRAME]: parse every animation, compare a pose to the rest pose.
        "check" => {
            let dir = args[2].trim_end_matches(['\\', '/']).to_string();
            let model = tt_formats::ghg::Model::parse(&archive.read(&format!("{dir}\\{}", args[3]))?)?;
            let prefix = format!("{}\\", dir.to_ascii_lowercase());
            let (mut ok, mut bad) = (0, 0);
            for path in archive.paths().filter(|p| p.to_ascii_lowercase().starts_with(&prefix) && p.ends_with(".an3")) {
                match tt_formats::an3::Animation::parse(&archive.read(path)?) {
                    Ok(a) if a.node_count() == model.joints.len() => ok += 1,
                    Ok(a) => {
                        bad += 1;
                        println!("{path}: {} nodes for {} joints", a.node_count(), model.joints.len());
                    }
                    Err(e) => {
                        bad += 1;
                        println!("{path}: {e}");
                    }
                }
            }
            println!("animations: {ok} ok, {bad} rejected");
            for hash in &model.texture_hashes {
                let path = format!("sharedtextures\\0x{hash:08x}.atitc");
                match archive.read(&path).and_then(|d| tt_formats::atc::decode(&d)) {
                    Ok(t) => println!("{path}: {}x{} alpha {} mips {}", t.width, t.height, t.has_alpha, t.mip_count),
                    Err(e) => println!("{path}: {e}"),
                }
            }
            if let (Some(name), Some(frame)) = (args.get(4), args.get(5)) {
                let anim = tt_formats::an3::Animation::parse(&archive.read(&format!("{dir}\\{name}.an3"))?)?;
                anim.print_summary();
                let frame: f32 = frame.parse().unwrap_or(0.0);
                let world = tt_formats::an3::world_matrices(&model.joints, &anim.sample_nodes(frame));
                for (joint, m) in model.joints.iter().zip(&world) {
                    let b = &joint.world_bind;
                    println!(
                        "{:18} anim ({:7.4}, {:7.4}, {:7.4})   rest ({:7.4}, {:7.4}, {:7.4})",
                        joint.name, m[12], m[13], m[14], b[12], b[13], b[14]
                    );
                }
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for layer in [0usize, 2, 5].iter().filter_map(|&i| model.layers.get(i)) {
                    for piece in &layer.pieces {
                        for v in piece.meshes.iter().flat_map(|m| &m.vertices) {
                            let p = match piece.rigid_joint {
                                Some(j) => tt_formats::ghg::transform_point(&world[j], v.position),
                                None => {
                                    let mut p = [0.0f32; 3];
                                    for (j, w) in v.weights {
                                        let j = j as usize;
                                        let skin = tt_formats::ghg::mul(&model.joints[j].inverse_bind, &world[j]);
                                        let q = tt_formats::ghg::transform_point(&skin, v.position);
                                        for k in 0..3 {
                                            p[k] += q[k] * w;
                                        }
                                    }
                                    p
                                }
                            };
                            for k in 0..3 {
                                lo[k] = lo[k].min(p[k]);
                                hi[k] = hi[k].max(p[k]);
                            }
                        }
                    }
                }
                println!("posed bounds: {lo:?} .. {hi:?}");
            }
        }
        other => return Err(tt_formats::Error::new(format!("unknown command {other}"))),
    }
    Ok(())
}
