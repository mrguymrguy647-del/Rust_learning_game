#![allow(clippy::expect_used, clippy::unwrap_used)]
use studio_core::data::ContentLibrary;
use studio_core::settings::Difficulty;
use studio_core::sim::*;

#[test]
#[ignore]
fn probe() {
    let lib = ContentLibrary::embedded();
    for size in ProjectSize::ALL {
        for skill in [2.0f32, 3.0, 4.0, 5.0, 6.0, 8.0, 9.0] {
            let mut st = GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(7));
            st.studio.tier = 4;
            st.studio.money = 1_000_000_000;
            for s in st.staff.iter_mut() {
                s.programming = skill;
                s.design = skill;
                s.art = skill;
                s.audio = skill;
            }
            let genre = lib.genres.iter().find(|g| g.id == "puzzle").unwrap();
            let cfg = ProjectConfig {
                name: "X".into(),
                genre: "puzzle".into(),
                theme: "cooking".into(),
                platform: "pc".into(),
                audience: Audience::Everyone,
                size,
                focus: genre.ideal,
                sequel_of: None,
            };
            st.start_project(&lib, cfg).unwrap();
            for _ in 0..2000 {
                if let Some(a) = st.pending_attempt.clone() {
                    let c = lib.challenge(&a.challenge_id).unwrap().clone();
                    let mut a2 = a.clone();
                    a2.submissions = 1;
                    st.complete_challenge(&lib, &c, &a2);
                    st.pending_attempt = None;
                    continue;
                }
                if st.project.as_ref().is_some_and(|p| p.is_complete() && p.open_blockers() == 0) {
                    break;
                }
                st.advance_week(&lib);
            }
            let idx = match st.release_project(&lib) {
                Ok(i) => i,
                Err(e) => {
                    println!(
                        "{size:?} skill {skill}: ERR {e} work {:?}",
                        st.project.as_ref().map(|p| (p.work_done, p.work_total, p.open_blockers()))
                    );
                    continue;
                }
            };
            let g = &st.games[idx];
            println!(
                "{:?} skill {skill}: quality {:.0} meta {:.0} bugs {:.0}  launch_units {:.0}",
                size, g.quality, g.metascore, g.bugs, g.launch_units
            );
        }
    }
}
