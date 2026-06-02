use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FactoryError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    WasmHashNotSet = 3,
    CampaignNotFound = 4,
}
