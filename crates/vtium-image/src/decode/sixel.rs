//! The sixel decoder will live here. The sixel handler's work fills it
//! with the decoder that turns a DCS `q` string's data, a byte at a time,
//! into premultiplied pixels (ADR-1100, D7).
