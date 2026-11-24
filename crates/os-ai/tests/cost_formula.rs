//! M4-K05: Kostenformel und Reservierungsbetrag gegen die Testpreise (AI-010 als Eingabe, AI-012 – AI-014).
#[path = "cost_support.rs"]
mod support;

use os_ai::cost::rates::{
    fraction_to_ppm, parse_decimal_scaled, usd_per_million_to_pico, usd_to_pico,
};
use os_ai::cost::reserve::{reservation_amount, reserved_input_tokens};
use os_ai::cost::{cost_of, CostBreakdown, FeeCounts, TokenCounts};
use os_model::budget::MicroUsd;
use proptest::prelude::*;
use support::{openrouter_rates, test_rates};

fn one() -> FeeCounts {
    FeeCounts::one_request(0)
}

// spec: AI-012
#[test]
fn endpoint_prices_from_the_test_price_file() {
    let responses = test_rates("openai", "gpt-6-luna", "responses", false);
    let decisions = test_rates("openai", "gpt-6-luna", "decisions", false);
    // 1.000 Eingabe + 200 Ausgabe über responses = 100 + 100 µUSD; dieselbe Eingabe über decisions = 100 µUSD.
    assert_eq!(
        cost_of(&TokenCounts::plain(1_000, 200), one(), &responses, false).cost,
        MicroUsd(200)
    );
    assert_eq!(
        cost_of(&TokenCounts::plain(1_000, 200), one(), &decisions, false).cost,
        MicroUsd(100)
    );
    // mock-claude 1/5 USD je 1 Mio.: 1.000 + 200 → 1.000 + 1.000 µUSD.
    let claude = test_rates("anthropic", "mock-claude", "messages", false);
    assert_eq!(
        cost_of(&TokenCounts::plain(1_000, 200), one(), &claude, false),
        CostBreakdown {
            token_cost: MicroUsd(2_000),
            fixed_fees: MicroUsd(0),
            regional_uplift: MicroUsd(0),
            cost: MicroUsd(2_000)
        }
    );
}

// spec: AI-012
#[test]
fn long_context_factors_and_regional_uplift_apply_to_the_whole_amount() {
    let global = test_rates("openai", "gpt-6-luna", "responses", false);
    let eu = test_rates("openai", "gpt-6-luna", "responses", true);
    let long = TokenCounts::plain(300_000, 1_000);
    // 300.000 × 0,20 + 1.000 × 0,75 je 1 Mio. = 60.750 µUSD.
    assert_eq!(cost_of(&long, one(), &global, false).cost, MicroUsd(60_750));
    let c = cost_of(&long, one(), &eu, false);
    assert_eq!(
        (c.token_cost, c.regional_uplift, c.cost),
        (MicroUsd(60_750), MicroUsd(6_075), MicroUsd(66_825))
    );
    // Schwelle einschließlich: ab 272.000 Eingabe-Tokens.
    assert_eq!(
        cost_of(&TokenCounts::plain(272_000, 0), one(), &global, false).cost,
        MicroUsd(54_400)
    );
    assert_eq!(
        cost_of(&TokenCounts::plain(271_999, 0), one(), &global, false).cost,
        MicroUsd(27_200)
    );
}

// spec: AI-012
#[test]
fn batch_discount_and_cache_reads_use_their_own_rates() {
    let r = test_rates("openai", "gpt-6-luna", "responses", false);
    // Batch 50 %: 1.000/200 → 0,05/0,25 je 1 Mio. = 50 + 50.
    assert_eq!(
        cost_of(&TokenCounts::plain(1_000, 200), one(), &r, true).cost,
        MicroUsd(100)
    );
    // 10.000 Eingabe, davon 8.000 aus dem Cache (0,01): 2.000 × 0,10 + 8.000 × 0,01 = 200 + 80.
    let cached = TokenCounts {
        input_tokens: 10_000,
        cached_input_tokens: 8_000,
        ..TokenCounts::default()
    };
    assert_eq!(cost_of(&cached, one(), &r, false).cost, MicroUsd(280));
}

// spec: AI-012
// spec: AI-014
#[test]
fn openrouter_request_and_image_fees_are_fixed_fees() {
    let vision = openrouter_rates("mock/vision");
    // 1 Request mit 1 Bild: Fixed Fees = 0.003000 USD.
    let c = cost_of(
        &TokenCounts::plain(100, 10),
        FeeCounts::one_request(1),
        &vision,
        false,
    );
    assert_eq!(c.fixed_fees, MicroUsd(3_000));
    assert_eq!(c.token_cost, MicroUsd(150), "100 × 1 + 10 × 5 µUSD");
    assert_eq!(c.cost, MicroUsd(3_150));
    // 3 Bilder in 3 Requests: 3 × (0,001 + 0,002) = 9.000 µUSD Gebühren.
    let fees = cost_of(
        &TokenCounts::default(),
        FeeCounts {
            requests: 3,
            images: 3,
        },
        &vision,
        false,
    );
    assert_eq!(fees.fixed_fees, MicroUsd(9_000));
    // Batch-Rabatt gilt nur für Tokenkosten.
    let r = os_ai::cost::Rates {
        batch_discount_ppm: 500_000,
        ..vision
    };
    assert_eq!(
        cost_of(
            &TokenCounts::plain(100, 10),
            FeeCounts::one_request(1),
            &r,
            true
        )
        .cost,
        MicroUsd(3_075)
    );
}

// spec: AI-013
#[test]
fn reservation_follows_the_ai_013_formula() {
    let claude = test_rates("anthropic", "mock-claude", "messages", false);
    // ⌈800 × 1,25⌉ = 1.000 Eingabe × 1 + max_tokens 600 × 5 = 4.000 µUSD.
    assert_eq!(reserved_input_tokens(800), 1_000);
    assert_eq!(
        reservation_amount(800, 600, 0, &claude, false),
        MicroUsd(4_000)
    );
    // Gebühren je Request und Bild werden reserviert.
    let vision = openrouter_rates("mock/vision");
    assert!(reservation_amount(0, 0, 1, &vision, false) >= MicroUsd(3_000));
    assert_eq!(reservation_amount(0, 0, 1, &vision, false), MicroUsd(3_000));
    // Langkontext, sobald geschätzt × 1,25 die Schwelle erreicht (217.600 × 1,25 = 272.000).
    let r = test_rates("openai", "gpt-6-luna", "responses", true);
    assert_eq!(
        reservation_amount(217_600, 0, 0, &r, false),
        MicroUsd(54_400 + 5_440)
    );
    assert_eq!(
        reservation_amount(217_599, 0, 0, &r, false),
        MicroUsd(27_200 + 2_720)
    );
    // Batch mit Rabatt.
    assert_eq!(
        reservation_amount(
            800,
            200,
            0,
            &test_rates("openai", "gpt-6-luna", "responses", false),
            true
        ),
        MicroUsd(100)
    );
}

// spec: AI-012
#[test]
fn decimal_prices_are_parsed_exactly_without_floating_point() {
    assert_eq!(usd_per_million_to_pico("0.1"), Some(100_000));
    assert_eq!(usd_per_million_to_pico("0.01"), Some(10_000));
    assert_eq!(usd_per_million_to_pico("5"), Some(5_000_000));
    assert_eq!(usd_to_pico("0.000001"), Some(1_000_000));
    assert_eq!(usd_to_pico("1e-6"), Some(1_000_000));
    assert_eq!(usd_to_pico("0.001"), Some(1_000_000_000));
    assert_eq!(fraction_to_ppm("0.5"), Some(500_000));
    assert_eq!(fraction_to_ppm("1.5"), Some(1_500_000));
    assert_eq!(parse_decimal_scaled("-1", 6), None);
    assert_eq!(parse_decimal_scaled("abc", 6), None);
    assert_eq!(parse_decimal_scaled("", 6), None);
}

fn rates_strategy() -> impl Strategy<Value = os_ai::cost::Rates> {
    (
        (
            0u64..20_000_000,
            0u64..80_000_000,
            0u64..2_000_000,
            0u64..10_000_000,
        ),
        (
            0u64..=1_000_000,
            0u64..300_000,
            0u64..5_000_000_000,
            0u64..5_000_000_000,
        ),
        proptest::option::of((
            1u64..500_000,
            1_000_000u64..3_000_000,
            1_000_000u64..2_000_000,
        )),
    )
        .prop_map(
            |((i, o, cr, cw), (batch, uplift, req, img), lc)| os_ai::cost::Rates {
                input_pico: i,
                output_pico: o,
                cache_read_pico: cr,
                cache_write_pico: cw,
                batch_discount_ppm: batch,
                long_context: lc.map(|(t, fi, fo)| os_ai::cost::LongContext {
                    threshold_input_tokens: t,
                    input_factor_ppm: fi,
                    output_factor_ppm: fo,
                    cache_read_factor_ppm: 1_000_000,
                }),
                regional_uplift_ppm: uplift,
                per_request_pico: req,
                per_image_pico: img,
                image_token_estimate: 85,
                source: os_ai::cost::PriceSource::Table,
            },
        )
}

proptest! {
    // spec: AI-014
    /// `Cost = Token Cost + Fixed Fees + Regional Uplift` für beliebige Preise, Tokens, Bilder, Batch und Region.
    #[test]
    fn cost_is_the_sum_of_its_three_parts(
        rates in rates_strategy(),
        input in 0u64..2_000_000, cached in 0u64..2_000_000, write in 0u64..100_000,
        output in 0u64..200_000, images in 0u64..20, batch in any::<bool>(),
    ) {
        let t = TokenCounts { input_tokens: input, cached_input_tokens: cached, cache_write_tokens: write, output_tokens: output };
        let c = cost_of(&t, FeeCounts::one_request(images), &rates, batch);
        prop_assert_eq!(c.cost.0, c.token_cost.0 + c.fixed_fees.0 + c.regional_uplift.0);
    }

    // spec: AI-013
    /// Liegen Ist-Tokens innerhalb der Reservierung (Eingabe ≤ geschätzt × 1,25, Ausgabe ≤ max_tokens, ohne
    /// Cache-Schreiben), liegen die Ist-Kosten nie über dem reservierten Betrag.
    #[test]
    fn actual_cost_within_reserved_tokens_never_exceeds_the_reservation(
        rates in rates_strategy(),
        estimated in 0u64..400_000, max_tokens in 0u64..50_000, images in 0u64..5, batch in any::<bool>(),
        input_share in 0u64..=1_000, cached_share in 0u64..=1_000, output_share in 0u64..=1_000,
    ) {
        let reserved_in = reserved_input_tokens(estimated);
        let input = reserved_in * input_share / 1_000;
        let t = TokenCounts {
            input_tokens: input,
            cached_input_tokens: input * cached_share / 1_000,
            cache_write_tokens: 0,
            output_tokens: max_tokens * output_share / 1_000,
        };
        // Cache-Lesen ist nie teurer als normale Eingabe (Preistabelle).
        let rates = os_ai::cost::Rates { cache_read_pico: rates.cache_read_pico.min(rates.input_pico), ..rates };
        let actual = cost_of(&t, FeeCounts::one_request(images), &rates, batch).cost;
        prop_assert!(actual <= reservation_amount(estimated, max_tokens, images, &rates, batch));
    }
}
