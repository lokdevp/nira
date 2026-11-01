//! UI strings via Fluent. Catalogs are `i18n/<locale>/nira.ftl`, embedded at
//! build time. `en-US` is complete; any other locale falls back to it id by
//! id.
//!
//! Call `init` once at startup, then `t("id")` or
//! `t_args("id", &[("count", 3.into())])`.

use std::sync::OnceLock;

use fluent_bundle::concurrent::FluentBundle;
pub use fluent_bundle::FluentValue;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

mod catalog {
    include!(concat!(env!("OUT_DIR"), "/i18n_catalog.rs"));
}

const FALLBACK: &str = "en-US";

/// The chosen locale's bundle first, then the fallback.
struct Translations {
    bundles: Vec<FluentBundle<FluentResource>>,
}

static TRANSLATIONS: OnceLock<Translations> = OnceLock::new();

/// Locales with an embedded catalog, sorted.
pub fn available_locales() -> Vec<&'static str> {
    catalog::CATALOG.iter().map(|(l, _)| *l).collect()
}

/// The catalog to use for `pref` (`"auto"` or a locale such as `de` or
/// `pt-BR`), given the system locale. Matches the full locale first, then
/// the language alone; `en-US` if nothing matches.
pub fn resolve_locale(pref: &str, system: Option<&str>) -> &'static str {
    let wanted = if pref.is_empty() || pref == "auto" {
        system.unwrap_or(FALLBACK)
    } else {
        pref
    };
    // "de_DE.UTF-8" → "de-DE"
    let wanted = wanted
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .replace('_', "-");
    let Ok(id) = wanted.parse::<LanguageIdentifier>() else {
        return FALLBACK;
    };
    let locales = available_locales();
    let parsed = |l: &str| l.parse::<LanguageIdentifier>().ok();
    locales
        .iter()
        .find(|l| parsed(l).as_ref() == Some(&id))
        .or_else(|| {
            locales
                .iter()
                .find(|l| parsed(l).is_some_and(|p| p.language == id.language))
        })
        .copied()
        .unwrap_or(FALLBACK)
}

fn bundle(locale: &str) -> Option<FluentBundle<FluentResource>> {
    let (_, text) = catalog::CATALOG.iter().find(|(l, _)| *l == locale)?;
    let res = FluentResource::try_new(text.to_string()).unwrap_or_else(|(res, errors)| {
        log::warn!("i18n: {locale}: {errors:?}");
        res
    });
    let mut bundle = FluentBundle::new_concurrent(vec![locale.parse().ok()?]);
    // GTK labels don't need bidi isolation marks around placeables.
    bundle.set_use_isolating(false);
    if let Err(errors) = bundle.add_resource(res) {
        log::warn!("i18n: {locale}: {errors:?}");
    }
    Some(bundle)
}

/// Load the translations for `language` (a setting value: `"auto"` or a
/// locale). Later calls are ignored.
pub fn init(language: &str) {
    let system = sys_locale::get_locale();
    let locale = resolve_locale(language, system.as_deref());
    log::info!("i18n: using {locale}");
    let _ = TRANSLATIONS.set(load(locale));
}

fn load(locale: &str) -> Translations {
    let mut bundles: Vec<_> = bundle(locale).into_iter().collect();
    if locale != FALLBACK {
        bundles.extend(bundle(FALLBACK));
    }
    Translations { bundles }
}

impl Translations {
    fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        for bundle in &self.bundles {
            let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) else {
                continue;
            };
            let mut errors = Vec::new();
            let text = bundle.format_pattern(pattern, args, &mut errors);
            if !errors.is_empty() {
                log::warn!("i18n: {id}: {errors:?}");
            }
            return text.into_owned();
        }
        log::warn!("i18n: missing string {id}");
        id.to_string()
    }
}

fn translations() -> &'static Translations {
    TRANSLATIONS.get_or_init(|| load(FALLBACK))
}

/// The string for `id` (the id itself if no catalog has it).
pub fn t(id: &str) -> String {
    translations().format(id, None)
}

/// The string for `id` with Fluent arguments filled in.
pub fn t_args(id: &str, args: &[(&str, FluentValue)]) -> String {
    let mut fargs = FluentArgs::new();
    for (name, value) in args {
        fargs.set(*name, value.clone());
    }
    translations().format(id, Some(&fargs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Action;

    #[test]
    fn resolves_locales() {
        assert_eq!(resolve_locale("auto", Some("en_US.UTF-8")), "en-US");
        assert_eq!(resolve_locale("auto", Some("en-GB")), "en-US");
        assert_eq!(resolve_locale("auto", None), "en-US");
        assert_eq!(resolve_locale("xx", None), "en-US");
        assert_eq!(resolve_locale("", Some("garbage!")), "en-US");
    }

    #[test]
    fn formats_plain_and_plural_strings() {
        let tr = load("en-US");
        assert_eq!(tr.format("fit-width", None), "Fit width");
        let mut args = FluentArgs::new();
        args.set("count", 1);
        assert_eq!(tr.format("pages-count", Some(&args)), "1 page");
        args.set("count", 12);
        assert_eq!(tr.format("pages-count", Some(&args)), "12 pages");
        assert_eq!(tr.format("no-such-id", None), "no-such-id");
    }

    #[test]
    fn every_catalog_parses_and_every_action_has_a_label() {
        for (locale, text) in catalog::CATALOG {
            if let Err((_, errors)) = FluentResource::try_new(text.to_string()) {
                panic!("{locale}: {errors:?}");
            }
        }
        let tr = load("en-US");
        for a in Action::ALL {
            let id = format!("action-{}", a.name());
            assert!(tr.bundles[0].has_message(&id), "missing {id}");
        }
    }
}
