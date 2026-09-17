use darling::{ast, FromDeriveInput, FromVariant};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derive `IntoResponse` for an error enum, mapping each variant to an
/// RFC 9457 problem details response.
///
/// # Attributes
///
/// Apply `#[problem(...)]` to each variant:
///
/// - `status = <u16>` — **required.** The HTTP status code for this variant.
/// - `title = "<str>"` — Optional. Defaults to the standard title for the
///   status code (e.g. 404 → "Not Found").
/// - `mask` — Optional. Hides the error detail from the HTTP response and
///   logs it via `tracing::error!` instead. Use for 5xx errors where you
///   don't want internal details leaking to clients.
///
/// # Example
///
/// ```rust,ignore
/// use axum_problem::AxumProblem;
/// use thiserror::Error;
///
/// #[derive(Debug, Error, AxumProblem)]
/// pub enum ApiError {
///     #[error("order {id} not found")]
///     #[problem(status = 404)]
///     NotFound { id: i64 },
///
///     #[error("access denied")]
///     #[problem(status = 401)]
///     Unauthorized,
///
///     #[error("db error: {0}")]
///     #[problem(status = 500, mask)]
///     Database(String),
/// }
/// ```
#[proc_macro_derive(AxumProblem, attributes(problem))]
pub fn derive_axum_problem(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match ProblemInput::from_derive_input(&input) {
        Ok(parsed) => expand_derive(parsed).into(),
        Err(e) => e.write_errors().into(),
    }
}

// ── Darling attribute parsing ─────────────────────────────────────────────────

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(problem), supports(enum_any))]
struct ProblemInput {
    ident: syn::Ident,
    generics: syn::Generics,
    data: ast::Data<ProblemVariant, ()>,
}

#[derive(Debug, FromVariant)]
#[darling(attributes(problem))]
struct ProblemVariant {
    ident: syn::Ident,
    fields: ast::Fields<syn::Type>,

    /// HTTP status code — required on every variant.
    status: u16,

    /// Optional custom title. Defaults to the standard title for the status.
    #[darling(default)]
    title: Option<String>,

    /// If set, hides the error detail from the HTTP response and logs it.
    /// Prevents internal details (DB errors, etc.) leaking to API clients.
    #[darling(default)]
    mask: bool,
}

// ── Code generation ───────────────────────────────────────────────────────────

fn expand_derive(input: ProblemInput) -> proc_macro2::TokenStream {
    let ProblemInput { ident, generics, data } = input;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let variants = match data {
        ast::Data::Enum(variants) => variants,
        _ => unreachable!("darling ensures enum_any"),
    };

    let arms = variants.iter().map(|v| build_match_arm(v, &ident));

    quote! {
        #[automatically_derived]
        impl #impl_generics ::axum::response::IntoResponse for #ident #ty_generics #where_clause {
            fn into_response(self) -> ::axum::response::Response {
                match self {
                    #(#arms),*
                }
            }
        }
    }
}

fn build_match_arm(variant: &ProblemVariant, enum_ident: &syn::Ident) -> proc_macro2::TokenStream {
    let v_ident = &variant.ident;
    let status = variant.status;
    let mask = variant.mask;

    // Custom title or auto from status code
    let title_expr = match &variant.title {
        Some(t) => quote! { #t.to_owned() },
        None => quote! { ::axum_problem::status_title(#status).to_owned() },
    };

    // Pattern to match the variant and capture fields
    let (pattern, display_expr) = build_pattern(enum_ident, variant);

    if mask {
        // Masked: log the full error, return generic message to client
        quote! {
            #pattern => {
                // Log the full error so it appears in tracing spans / log aggregators
                ::tracing::error!(
                    error = %#display_expr,
                    status = #status,
                    "masked error — full detail suppressed in HTTP response"
                );
                let problem = ::axum_problem::Problem::new(#status)
                    .title(#title_expr);
                ::axum::response::IntoResponse::into_response(problem)
            }
        }
    } else {
        // Not masked: include the Display impl as the detail field
        quote! {
            #pattern => {
                let detail = format!("{}", #display_expr);
                let problem = ::axum_problem::Problem::new(#status)
                    .title(#title_expr)
                    .detail(detail);
                ::axum::response::IntoResponse::into_response(problem)
            }
        }
    }
}

fn build_pattern(
    enum_ident: &syn::Ident,
    variant: &ProblemVariant,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let v_ident = &variant.ident;

    match &variant.fields.style {
        ast::Style::Unit => {
            // Unit variant: MyError::Foo
            let pattern = quote! { #enum_ident::#v_ident };
            // For the display expression, use the enum value itself
            let display = quote! { #enum_ident::#v_ident };
            (pattern, display)
        }
        ast::Style::Tuple => {
            // Use ref-binding on the whole enum so Display uses the thiserror
            // template (e.g. "validation: {0}"), not just the raw inner value.
            // `(..)` matches any number of fields, including zero.
            let pattern = quote! { ref __self @ #enum_ident::#v_ident(..) };
            let display = quote! { __self };
            (pattern, display)
        }
        ast::Style::Struct => {
            // Struct variant: bind by ref and use Display from the enum value
            let pattern = quote! { ref __self @ #enum_ident::#v_ident { .. } };
            let display = quote! { __self };
            (pattern, display)
        }
    }
}
