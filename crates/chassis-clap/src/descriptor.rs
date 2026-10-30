use std::ffi::CStr;

use clack_plugin::prelude::PluginDescriptor;

/// Author-supplied metadata for one CLAP export.
///
/// Declare this through [`ClapComponent::CLAP_DESCRIPTOR`](crate::ClapComponent::CLAP_DESCRIPTOR).
/// Optional strings default to empty (unset); no vendor, version, URLs, or
/// features are inferred. The ID is deployment identity, independent of the
/// component schema's semantic state identity.
///
/// Strings must not contain NUL bytes, and ID and name must not be empty.
/// Clack validates these requirements when constructing the factory descriptor
/// and owns its C-string storage for the factory's lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClapDescriptor {
    /// Stable reverse-domain CLAP plugin identifier.
    pub id: &'static str,
    /// Human-readable plugin name.
    pub name: &'static str,
    /// CLAP feature strings, without inferred product roles.
    pub features: &'static [&'static CStr],
    /// Vendor display name, or empty when unset.
    pub vendor: &'static str,
    /// Product version, or empty when unset.
    pub version: &'static str,
    /// Short product description, or empty when unset.
    pub description: &'static str,
    /// Product homepage URL, or empty when unset.
    pub url: &'static str,
    /// User manual URL, or empty when unset.
    pub manual_url: &'static str,
    /// Support URL, or empty when unset.
    pub support_url: &'static str,
}

impl ClapDescriptor {
    /// Declare an export with all optional metadata unset.
    ///
    /// ```
    /// use chassis_clap::ClapDescriptor;
    ///
    /// const DESCRIPTOR: ClapDescriptor = ClapDescriptor {
    ///     vendor: "Example Audio",
    ///     version: env!("CARGO_PKG_VERSION"),
    ///     features: &[c"audio-effect", c"stereo"],
    ///     ..ClapDescriptor::new("com.example.echo", "Echo")
    /// };
    /// ```
    #[must_use]
    pub const fn new(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            features: &[],
            vendor: "",
            version: "",
            description: "",
            url: "",
            manual_url: "",
            support_url: "",
        }
    }

    pub(crate) fn into_plugin_descriptor(self) -> PluginDescriptor {
        PluginDescriptor::new(self.id, self.name)
            .with_features(self.features.iter().copied())
            .with_vendor(self.vendor)
            .with_version(self.version)
            .with_description(self.description)
            .with_url(self.url)
            .with_manual_url(self.manual_url)
            .with_support_url(self.support_url)
    }
}
