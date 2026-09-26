# axum-problem — improvement to-do

Current downloads: 105 (axum-problem), 73 (axum-problem-derive)

## Quick wins (15 min each)

- [ ] `axum-problem/Cargo.toml`: change category `"rust-patterns"` → `"api-bindings"`
      — that's where axum users actually browse; "rust-patterns" gets no relevant traffic
- [ ] `axum-problem/Cargo.toml`: add keyword `"problem-json"`
      — what RFC 9457 implementors actually search (currently at limit, drop `"derive"` since it's obvious)

## Medium effort (1–2 hours)

- [ ] Add "vs alternatives" section to README (~20 lines)
      — compare vs `tower-http` error layers, the `problem` crate, and manual IntoResponse
      — yours wins on derive-macro ergonomics; make that case explicitly
- [ ] Track axum 0.8 compatibility — pin a GitHub milestone/issue so it surfaces in searches
      — axum 0.8 will be a migration wave; being the first compatible `axum-problem` release wins installs

## Structural

- [ ] Verify `axum-problem-derive` re-export is obvious from `axum-problem` docs
      — users shouldn't need to know the derive crate exists separately
