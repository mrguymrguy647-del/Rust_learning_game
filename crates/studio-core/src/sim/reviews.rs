//! Fictional review outlets turn quality numbers into scores and quotes.

use super::library::Review;
use super::model::{Category, Weights};
use super::quality::QualityReport;
use super::rng::GameRng;
use crate::data::{ContentLibrary, OutletDef};

/// The category the game is best / worst at among those the player actually invested in.
pub fn strongest_and_weakest(categories: &Weights, focus: &Weights) -> (Category, Category) {
    let invested: Vec<Category> = Category::ALL.iter().copied().filter(|c| focus.get(*c) >= 10.0).collect();
    let pool: &[Category] = if invested.is_empty() { &Category::ALL } else { &invested };
    let strong = pool.iter().copied().max_by(|a, b| categories.get(*a).total_cmp(&categories.get(*b)));
    let weak = pool.iter().copied().min_by(|a, b| categories.get(*a).total_cmp(&categories.get(*b)));
    (strong.unwrap_or(Category::Gameplay), weak.unwrap_or(Category::Gameplay))
}

fn fill(template: &str, game: &str, genre: &str, strong: Category, weak: Category) -> String {
    template
        .replace("{game}", game)
        .replace("{genre}", &genre.to_lowercase())
        .replace("{strong}", &strong.label().to_lowercase())
        .replace("{weak}", &weak.label().to_lowercase())
}

/// Capitalise the first letter of the text and of every sentence ("… out. audio drags" -> "… out. Audio drags").
pub fn capitalize_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start_of_sentence = true;
    for ch in text.chars() {
        if start_of_sentence && ch.is_alphabetic() {
            out.extend(ch.to_uppercase());
            start_of_sentence = false;
        } else {
            out.push(ch);
            if matches!(ch, '.' | '!' | '?') {
                start_of_sentence = true;
            } else if !ch.is_whitespace() && !matches!(ch, '“' | '"' | '(') {
                start_of_sentence = false;
            }
        }
    }
    out
}

fn round_half(x: f32) -> f32 {
    (x * 2.0).round() / 2.0
}

pub fn score_for(outlet: &OutletDef, q: &QualityReport, rng: &mut GameRng) -> f32 {
    let wsum = outlet.weights.sum().max(0.0001);
    let raw: f32 = Category::ALL.iter().map(|c| outlet.weights.get(*c) / wsum * q.categories.get(*c)).sum();
    let blend = 0.5 * raw + 0.5 * q.overall;
    let s = blend / 10.0 + outlet.harshness - q.bug_penalty * 4.0 + rng.noise(0.3);
    round_half(s.clamp(1.0, 10.0))
}

/// Generate one review per outlet. Returns the reviews and the 0..=100 metascore.
pub fn generate(
    content: &ContentLibrary,
    q: &QualityReport,
    focus: &Weights,
    game: &str,
    genre_name: &str,
    rng: &mut GameRng,
) -> (Vec<Review>, f32) {
    let (strong, weak) = strongest_and_weakest(&q.categories, focus);
    let mut reviews = Vec::new();
    for outlet in &content.outlets {
        let score = score_for(outlet, q, rng);
        let pool: &[String] = if score >= 8.5 {
            &outlet.quotes_great
        } else if score >= 6.5 {
            &outlet.quotes_good
        } else if score >= 4.5 {
            &outlet.quotes_mixed
        } else {
            &outlet.quotes_bad
        };
        let mut quote = rng
            .pick(pool)
            .map(|t| fill(t, game, genre_name, strong, weak))
            .unwrap_or_else(|| format!("{game}: {score}/10."));
        if q.bugs > 22.0 {
            if let Some(extra) = rng.pick(&outlet.quotes_buggy) {
                quote.push(' ');
                quote.push_str(extra);
            }
        }
        reviews.push(Review {
            outlet_id: outlet.id.clone(),
            outlet: outlet.name.clone(),
            score,
            quote: capitalize_sentences(&quote),
        });
    }
    let meta = if reviews.is_empty() {
        q.overall
    } else {
        reviews.iter().map(|r| r.score).sum::<f32>() / reviews.len() as f32 * 10.0
    };
    (reviews, meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(overall: f32, bugs: f32) -> QualityReport {
        QualityReport {
            categories: Weights::new(overall, overall, overall, overall, overall),
            overall,
            bug_penalty: (bugs / 100.0 * 0.8).min(0.5),
            bugs,
            ..Default::default()
        }
    }

    #[test]
    fn better_games_get_better_scores_and_quotes_fill_placeholders() {
        let lib = ContentLibrary::load(None);
        let mut rng = GameRng::from_seed(3);
        let focus = Weights::EVEN;
        let (low, low_meta) = generate(&lib, &report(25.0, 5.0), &focus, "Dud", "Puzzle", &mut rng);
        let (high, high_meta) = generate(&lib, &report(92.0, 5.0), &focus, "Gem", "Puzzle", &mut rng);
        assert_eq!(low.len(), lib.outlets.len());
        assert!(high_meta > low_meta + 30.0, "{high_meta} vs {low_meta}");
        for r in low.iter().chain(&high) {
            assert!((1.0..=10.0).contains(&r.score));
            assert_eq!((r.score * 2.0).fract(), 0.0, "half-point steps");
            assert!(!r.quote.contains('{'), "unfilled placeholder in: {}", r.quote);
        }
    }

    #[test]
    fn sentences_get_capital_letters() {
        assert_eq!(
            capitalize_sentences("audio drags it down. performance is fine!"),
            "Audio drags it down. Performance is fine!"
        );
        assert_eq!(capitalize_sentences("“hello” world. ok?"), "“Hello” world. Ok?");
        assert_eq!(capitalize_sentences("Already Fine."), "Already Fine.");
    }

    #[test]
    fn bugs_hurt_scores() {
        let lib = ContentLibrary::load(None);
        let focus = Weights::EVEN;
        let clean = generate(&lib, &report(70.0, 2.0), &focus, "G", "Puzzle", &mut GameRng::from_seed(9)).1;
        let buggy = generate(&lib, &report(70.0, 50.0), &focus, "G", "Puzzle", &mut GameRng::from_seed(9)).1;
        assert!(clean > buggy + 8.0, "{clean} vs {buggy}");
    }

    #[test]
    fn strongest_ignores_categories_the_player_skipped() {
        let cats = Weights::new(40.0, 99.0, 50.0, 30.0, 60.0);
        let focus = Weights::new(40.0, 0.0, 10.0, 0.0, 50.0);
        let (strong, weak) = strongest_and_weakest(&cats, &focus);
        assert_eq!(strong, Category::Performance);
        assert_eq!(weak, Category::Gameplay);
    }
}
