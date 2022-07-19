pub(crate) struct SymmetricKey(pub(crate) Zeroizing<[u8; Self::LEN]>);

impl SymmetricKey {
    pub(crate) const LEN: usize = 64;
    pub(crate) fn stretch_master(master_key: &MasterKey) -> Self {
        let mut key = Self::zeroed();
        let hkdf = <Hkdf<Sha256>>::from_prk(&*master_key.0).unwrap();
        hkdf.expand(b"enc", &mut key.0[0..32]).unwrap();
        hkdf.expand(b"mac", &mut key.0[32..64]).unwrap();
        key
    }
    pub(crate) fn zeroed() -> Self {
        Self(Zeroizing::new([0; Self::LEN]))
    }
    pub(crate) fn encryption_key(&self) -> &[u8; 32] {
        self.0[0..32].try_into().unwrap()
    }
    pub(crate) fn mac_key(&self) -> &[u8; 32] {
        self.0[32..64].try_into().unwrap()
    }
}

use hkdf::Hkdf;
use rofi_bw_common::MasterKey;
use sha2::Sha256;
use zeroize::Zeroizing;
