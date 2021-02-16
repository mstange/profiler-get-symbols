use cpp_demangle;
use msvc_demangler;
use rustc_demangle;

use msvc_demangler::DemangleFlags;

pub fn demangle_any(name: &str) -> String {
    if name.starts_with('?') {
        let flags = DemangleFlags::NO_ACCESS_SPECIFIERS
            | DemangleFlags::NO_FUNCTION_RETURNS
            | DemangleFlags::NO_MEMBER_TYPE
            | DemangleFlags::NO_MS_KEYWORDS
            | DemangleFlags::NO_THISTYPE
            | DemangleFlags::NO_CLASS_TYPE
            | DemangleFlags::SPACE_AFTER_COMMA
            | DemangleFlags::HUG_TYPE;
        return msvc_demangler::demangle(&name, flags).unwrap_or_else(|_| name.to_string());
    }
    if let Ok(demangled_symbol) = rustc_demangle::try_demangle(name) {
        return format!("{:#}", demangled_symbol);
    }

    let options = cpp_demangle::DemangleOptions::default().no_return_type();
    if let Ok(symbol) = cpp_demangle::Symbol::new(name) {
        if let Ok(demangled_string) = symbol.demangle(&options) {
            return demangled_string;
        }
    }

    if name.starts_with('_') {
        return name.split_at(1).1.to_owned();
    }

    name.to_owned()
}
