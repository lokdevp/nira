//! Nira's core: everything that does not need GTK. Archives and page
//! decoding, the zoom and smart-scroll models, double-page ordering, the
//! background workers, and the files Nira stores (settings, keys,
//! bookmarks, reading history, thumbnail cache).

pub mod archive;
pub mod bookmarks;
pub mod history;
pub mod i18n;
pub mod image_loader;
pub mod keymap;
pub mod layout;
pub mod lru;
pub mod natsort;
pub mod ordering;
pub mod paths;
pub mod reader;
pub mod scroll;
pub mod settings;
pub mod siblings;
pub mod thumb_cache;
pub mod thumbs;
pub mod workers;
pub mod zoom;
