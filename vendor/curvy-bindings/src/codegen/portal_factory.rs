///Module containing a contract's types and functions.
/**

```solidity
library CurvyTypes {
    struct Note { uint256 ownerHash; uint256 token; uint256 amount; uint256[2] ephemeralKey; uint16 viewTag; }
}
```*/
#[allow(
    non_camel_case_types,
    non_snake_case,
    clippy::pub_underscore_fields,
    clippy::style,
    clippy::empty_structs_with_brackets
)]
pub mod CurvyTypes {
    use super::*;
    use alloy::sol_types as alloy_sol_types;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**```solidity
struct Note { uint256 ownerHash; uint256 token; uint256 amount; uint256[2] ephemeralKey; uint16 viewTag; }
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct Note {
        #[allow(missing_docs)]
        pub ownerHash: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub token: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub amount: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub ephemeralKey: [alloy::sol_types::private::primitives::aliases::U256; 2usize],
        #[allow(missing_docs)]
        pub viewTag: u16,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = (
            alloy::sol_types::sol_data::Uint<256>,
            alloy::sol_types::sol_data::Uint<256>,
            alloy::sol_types::sol_data::Uint<256>,
            alloy::sol_types::sol_data::FixedArray<
                alloy::sol_types::sol_data::Uint<256>,
                2usize,
            >,
            alloy::sol_types::sol_data::Uint<16>,
        );
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = (
            alloy::sol_types::private::primitives::aliases::U256,
            alloy::sol_types::private::primitives::aliases::U256,
            alloy::sol_types::private::primitives::aliases::U256,
            [alloy::sol_types::private::primitives::aliases::U256; 2usize],
            u16,
        );
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<Note> for UnderlyingRustTuple<'_> {
            fn from(value: Note) -> Self {
                (
                    value.ownerHash,
                    value.token,
                    value.amount,
                    value.ephemeralKey,
                    value.viewTag,
                )
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for Note {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self {
                    ownerHash: tuple.0,
                    token: tuple.1,
                    amount: tuple.2,
                    ephemeralKey: tuple.3,
                    viewTag: tuple.4,
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolValue for Note {
            type SolType = Self;
        }
        #[automatically_derived]
        impl alloy_sol_types::private::SolTypeValue<Self> for Note {
            #[inline]
            fn stv_to_tokens(&self) -> <Self as alloy_sol_types::SolType>::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.ownerHash),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.token),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.amount),
                    <alloy::sol_types::sol_data::FixedArray<
                        alloy::sol_types::sol_data::Uint<256>,
                        2usize,
                    > as alloy_sol_types::SolType>::tokenize(&self.ephemeralKey),
                    <alloy::sol_types::sol_data::Uint<
                        16,
                    > as alloy_sol_types::SolType>::tokenize(&self.viewTag),
                )
            }
            #[inline]
            fn stv_abi_encoded_size(&self) -> usize {
                if let Some(size) = <Self as alloy_sol_types::SolType>::ENCODED_SIZE {
                    return size;
                }
                let tuple = <UnderlyingRustTuple<
                    '_,
                > as ::core::convert::From<Self>>::from(self.clone());
                <UnderlyingSolTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_encoded_size(&tuple)
            }
            #[inline]
            fn stv_eip712_data_word(&self) -> alloy_sol_types::Word {
                <Self as alloy_sol_types::SolStruct>::eip712_hash_struct(self)
            }
            #[inline]
            fn stv_abi_encode_packed_to(
                &self,
                out: &mut alloy_sol_types::private::Vec<u8>,
            ) {
                let tuple = <UnderlyingRustTuple<
                    '_,
                > as ::core::convert::From<Self>>::from(self.clone());
                <UnderlyingSolTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_encode_packed_to(&tuple, out)
            }
            #[inline]
            fn stv_abi_packed_encoded_size(&self) -> usize {
                if let Some(size) = <Self as alloy_sol_types::SolType>::PACKED_ENCODED_SIZE {
                    return size;
                }
                let tuple = <UnderlyingRustTuple<
                    '_,
                > as ::core::convert::From<Self>>::from(self.clone());
                <UnderlyingSolTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_packed_encoded_size(&tuple)
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolType for Note {
            type RustType = Self;
            type Token<'a> = <UnderlyingSolTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SOL_NAME: &'static str = <Self as alloy_sol_types::SolStruct>::NAME;
            const ENCODED_SIZE: Option<usize> = <UnderlyingSolTuple<
                '_,
            > as alloy_sol_types::SolType>::ENCODED_SIZE;
            const PACKED_ENCODED_SIZE: Option<usize> = <UnderlyingSolTuple<
                '_,
            > as alloy_sol_types::SolType>::PACKED_ENCODED_SIZE;
            #[inline]
            fn valid_token(token: &Self::Token<'_>) -> bool {
                <UnderlyingSolTuple<'_> as alloy_sol_types::SolType>::valid_token(token)
            }
            #[inline]
            fn detokenize(token: Self::Token<'_>) -> Self::RustType {
                let tuple = <UnderlyingSolTuple<
                    '_,
                > as alloy_sol_types::SolType>::detokenize(token);
                <Self as ::core::convert::From<UnderlyingRustTuple<'_>>>::from(tuple)
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolStruct for Note {
            const NAME: &'static str = "Note";
            #[inline]
            fn eip712_root_type() -> alloy_sol_types::private::Cow<'static, str> {
                alloy_sol_types::private::Cow::Borrowed(
                    "Note(uint256 ownerHash,uint256 token,uint256 amount,uint256[2] ephemeralKey,uint16 viewTag)",
                )
            }
            #[inline]
            fn eip712_components() -> alloy_sol_types::private::Vec<
                alloy_sol_types::private::Cow<'static, str>,
            > {
                alloy_sol_types::private::Vec::new()
            }
            #[inline]
            fn eip712_encode_type() -> alloy_sol_types::private::Cow<'static, str> {
                <Self as alloy_sol_types::SolStruct>::eip712_root_type()
            }
            #[inline]
            fn eip712_encode_data(&self) -> alloy_sol_types::private::Vec<u8> {
                [
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::eip712_data_word(&self.ownerHash)
                        .0,
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::eip712_data_word(&self.token)
                        .0,
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::eip712_data_word(&self.amount)
                        .0,
                    <alloy::sol_types::sol_data::FixedArray<
                        alloy::sol_types::sol_data::Uint<256>,
                        2usize,
                    > as alloy_sol_types::SolType>::eip712_data_word(&self.ephemeralKey)
                        .0,
                    <alloy::sol_types::sol_data::Uint<
                        16,
                    > as alloy_sol_types::SolType>::eip712_data_word(&self.viewTag)
                        .0,
                ]
                    .concat()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::EventTopic for Note {
            #[inline]
            fn topic_preimage_length(rust: &Self::RustType) -> usize {
                0usize
                    + <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::EventTopic>::topic_preimage_length(
                        &rust.ownerHash,
                    )
                    + <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::EventTopic>::topic_preimage_length(&rust.token)
                    + <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::EventTopic>::topic_preimage_length(
                        &rust.amount,
                    )
                    + <alloy::sol_types::sol_data::FixedArray<
                        alloy::sol_types::sol_data::Uint<256>,
                        2usize,
                    > as alloy_sol_types::EventTopic>::topic_preimage_length(
                        &rust.ephemeralKey,
                    )
                    + <alloy::sol_types::sol_data::Uint<
                        16,
                    > as alloy_sol_types::EventTopic>::topic_preimage_length(
                        &rust.viewTag,
                    )
            }
            #[inline]
            fn encode_topic_preimage(
                rust: &Self::RustType,
                out: &mut alloy_sol_types::private::Vec<u8>,
            ) {
                out.reserve(
                    <Self as alloy_sol_types::EventTopic>::topic_preimage_length(rust),
                );
                <alloy::sol_types::sol_data::Uint<
                    256,
                > as alloy_sol_types::EventTopic>::encode_topic_preimage(
                    &rust.ownerHash,
                    out,
                );
                <alloy::sol_types::sol_data::Uint<
                    256,
                > as alloy_sol_types::EventTopic>::encode_topic_preimage(
                    &rust.token,
                    out,
                );
                <alloy::sol_types::sol_data::Uint<
                    256,
                > as alloy_sol_types::EventTopic>::encode_topic_preimage(
                    &rust.amount,
                    out,
                );
                <alloy::sol_types::sol_data::FixedArray<
                    alloy::sol_types::sol_data::Uint<256>,
                    2usize,
                > as alloy_sol_types::EventTopic>::encode_topic_preimage(
                    &rust.ephemeralKey,
                    out,
                );
                <alloy::sol_types::sol_data::Uint<
                    16,
                > as alloy_sol_types::EventTopic>::encode_topic_preimage(
                    &rust.viewTag,
                    out,
                );
            }
            #[inline]
            fn encode_topic(
                rust: &Self::RustType,
            ) -> alloy_sol_types::abi::token::WordToken {
                let mut out = alloy_sol_types::private::Vec::new();
                <Self as alloy_sol_types::EventTopic>::encode_topic_preimage(
                    rust,
                    &mut out,
                );
                alloy_sol_types::abi::token::WordToken(
                    alloy_sol_types::private::keccak256(out),
                )
            }
        }
    };
    use alloy::contract as alloy_contract;
    /**Creates a new wrapper around an on-chain [`CurvyTypes`](self) contract instance.

See the [wrapper's documentation](`CurvyTypesInstance`) for more details.*/
    #[inline]
    pub const fn new<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    >(
        address: alloy_sol_types::private::Address,
        __provider: P,
    ) -> CurvyTypesInstance<P, N> {
        CurvyTypesInstance::<P, N>::new(address, __provider)
    }
    /**A [`CurvyTypes`](self) instance.

Contains type-safe methods for interacting with an on-chain instance of the
[`CurvyTypes`](self) contract located at a given `address`, using a given
provider `P`.

If the contract bytecode is available (see the [`sol!`](alloy_sol_types::sol!)
documentation on how to provide it), the `deploy` and `deploy_builder` methods can
be used to deploy a new instance of the contract.

See the [module-level documentation](self) for all the available methods.*/
    #[derive(Clone)]
    pub struct CurvyTypesInstance<P, N = alloy_contract::private::Ethereum> {
        address: alloy_sol_types::private::Address,
        provider: P,
        _network: ::core::marker::PhantomData<N>,
    }
    #[automatically_derived]
    impl<P, N> ::core::fmt::Debug for CurvyTypesInstance<P, N> {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple("CurvyTypesInstance").field(&self.address).finish()
        }
    }
    /// Instantiation and getters/setters.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > CurvyTypesInstance<P, N> {
        /**Creates a new wrapper around an on-chain [`CurvyTypes`](self) contract instance.

See the [wrapper's documentation](`CurvyTypesInstance`) for more details.*/
        #[inline]
        pub const fn new(
            address: alloy_sol_types::private::Address,
            __provider: P,
        ) -> Self {
            Self {
                address,
                provider: __provider,
                _network: ::core::marker::PhantomData,
            }
        }
        /// Returns a reference to the address.
        #[inline]
        pub const fn address(&self) -> &alloy_sol_types::private::Address {
            &self.address
        }
        /// Sets the address.
        #[inline]
        pub fn set_address(&mut self, address: alloy_sol_types::private::Address) {
            self.address = address;
        }
        /// Sets the address and returns `self`.
        pub fn at(mut self, address: alloy_sol_types::private::Address) -> Self {
            self.set_address(address);
            self
        }
        /// Returns a reference to the provider.
        #[inline]
        pub const fn provider(&self) -> &P {
            &self.provider
        }
    }
    impl<P: ::core::clone::Clone, N> CurvyTypesInstance<&P, N> {
        /// Clones the provider and returns a new instance with the cloned provider.
        #[inline]
        pub fn with_cloned_provider(self) -> CurvyTypesInstance<P, N> {
            CurvyTypesInstance {
                address: self.address,
                provider: ::core::clone::Clone::clone(&self.provider),
                _network: ::core::marker::PhantomData,
            }
        }
    }
    /// Function calls.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > CurvyTypesInstance<P, N> {
        /// Creates a new call builder using this contract instance's provider and address.
        ///
        /// Note that the call can be any function call, not just those defined in this
        /// contract. Prefer using the other methods for building type-safe contract calls.
        pub fn call_builder<C: alloy_sol_types::SolCall>(
            &self,
            call: &C,
        ) -> alloy_contract::SolCallBuilder<&P, C, N> {
            alloy_contract::SolCallBuilder::new_sol(&self.provider, &self.address, call)
        }
    }
    /// Event filters.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > CurvyTypesInstance<P, N> {
        /// Creates a new event filter using this contract instance's provider and address.
        ///
        /// Note that the type can be any event, not just those defined in this contract.
        /// Prefer using the other methods for building type-safe event filters.
        pub fn event_filter<E: alloy_sol_types::SolEvent>(
            &self,
        ) -> alloy_contract::Event<&P, E, N> {
            alloy_contract::Event::new_sol(&self.provider, &self.address)
        }
    }
}
/**

Generated by the following Solidity interface...
```solidity
library CurvyTypes {
    struct Note {
        uint256 ownerHash;
        uint256 token;
        uint256 amount;
        uint256[2] ephemeralKey;
        uint16 viewTag;
    }
}

interface PortalFactory {
    error AccessControlBadConfirmation();
    error AccessControlUnauthorizedAccount(address account, bytes32 neededRole);
    error AmountMismatch();
    error DeploymentFailed();
    error FailedDeployment();
    error InsufficientBalance(uint256 balance, uint256 needed);
    error InvalidLiFiDestinationChain();
    error InvalidLiFiReceiver();
    error OwnableInvalidOwner(address owner);
    error OwnableUnauthorizedAccount(address account);
    error UnsupportedBridging();
    error UnsupportedShielding();

    event ConfigUpdated(address curvyVaultProxyAddress, address curvyAggregatorAlphaProxyAddress, address lifiDiamondAddress);
    event EntryBridgePortalDeployed(address indexed portalAddress, uint256 indexed ownerHash, address indexed recovery, address currency);
    event ExitBridgePortalDeployed(address indexed portalAddress, address indexed exitAddress, uint256 exitChainId, address indexed recovery, address currency);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event RecoveryPortalDeployed(address indexed portalAddress, address indexed tokenAddress, address indexed to);
    event RoleAdminChanged(bytes32 indexed role, bytes32 indexed previousAdminRole, bytes32 indexed newAdminRole);
    event RoleGranted(bytes32 indexed role, address indexed account, address indexed sender);
    event RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender);
    event ShieldPortalDeployed(address indexed portalAddress, uint256 indexed ownerHash, address indexed recovery);
    event SolanaExitBridgePortalDeployed(address indexed portalAddress, bytes32 indexed exitAddress, uint256 exitChainId, address indexed recovery, address currency);
    event SolanaRecoveryPortalDeployed(address indexed portalAddress, bytes32 indexed exitAddress, address indexed tokenAddress, address to);

    constructor(address initialOwner);

    function AUTHORITY_ROLE() external view returns (bytes32);
    function DEFAULT_ADMIN_ROLE() external view returns (bytes32);
    function OPERATOR_ROLE() external view returns (bytes32);
    function deployEntryBridgePortal(bytes memory bridgeData, CurvyTypes.Note memory note, address currency, address recovery, uint256 gasFee) external payable;
    function deployExitBridgePortal(bytes memory bridgeData, uint256 amount, address currency, address exitAddress, uint256 exitChainId, address recovery, uint256 gasFee) external payable;
    function deployRecoveryEntryPortal(uint256 ownerHash, address recovery, address tokenAddress, address to) external;
    function deployRecoveryExitPortal(address exitAddress, uint256 exitChainId, address recovery, address tokenAddress, address to) external;
    function deployShieldPortal(CurvyTypes.Note memory note, address recovery) external;
    function deploySolanaExitBridgePortal(bytes memory bridgeData, uint256 amount, address currency, bytes32 exitAddress, uint256 exitChainId, address recovery, uint256 gasFee) external payable;
    function deploySolanaRecoveryExitPortal(bytes32 exitAddress, uint256 exitChainId, address recovery, address tokenAddress, address to) external;
    function getEntryPortalAddress(uint256 ownerHash, address recovery) external view returns (address);
    function getExitPortalAddress(address exitAddress, uint256 exitChainId, address recovery) external view returns (address);
    function getRoleAdmin(bytes32 role) external view returns (bytes32);
    function getSolanaExitPortalAddress(bytes32 exitAddress, uint256 exitChainId, address recovery) external view returns (address);
    function grantRole(bytes32 role, address account) external;
    function hasRole(bytes32 role, address account) external view returns (bool);
    function owner() external view returns (address);
    function portalImpl() external view returns (address);
    function portalIsRegistered(address portalAddress) external view returns (bool);
    function renounceOwnership() external;
    function renounceRole(bytes32 role, address callerConfirmation) external;
    function revokeRole(bytes32 role, address account) external;
    function solanaPortalImpl() external view returns (address);
    function supportsInterface(bytes4 interfaceId) external view returns (bool);
    function transferOwnership(address newOwner) external;
    function updateConfig(address curvyVaultProxyAddress, address curvyAggregatorAlphaProxyAddress, address lifiDiamondAddress) external returns (bool);
}
```

...which was generated by the following JSON ABI:
```json
[
  {
    "type": "constructor",
    "inputs": [
      {
        "name": "initialOwner",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "AUTHORITY_ROLE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "DEFAULT_ADMIN_ROLE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "OPERATOR_ROLE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "deployEntryBridgePortal",
    "inputs": [
      {
        "name": "bridgeData",
        "type": "bytes",
        "internalType": "bytes"
      },
      {
        "name": "note",
        "type": "tuple",
        "internalType": "struct CurvyTypes.Note",
        "components": [
          {
            "name": "ownerHash",
            "type": "uint256",
            "internalType": "uint256"
          },
          {
            "name": "token",
            "type": "uint256",
            "internalType": "uint256"
          },
          {
            "name": "amount",
            "type": "uint256",
            "internalType": "uint256"
          },
          {
            "name": "ephemeralKey",
            "type": "uint256[2]",
            "internalType": "uint256[2]"
          },
          {
            "name": "viewTag",
            "type": "uint16",
            "internalType": "uint16"
          }
        ]
      },
      {
        "name": "currency",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "gasFee",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "deployExitBridgePortal",
    "inputs": [
      {
        "name": "bridgeData",
        "type": "bytes",
        "internalType": "bytes"
      },
      {
        "name": "amount",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "currency",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "exitAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "gasFee",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "deployRecoveryEntryPortal",
    "inputs": [
      {
        "name": "ownerHash",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "tokenAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "to",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "deployRecoveryExitPortal",
    "inputs": [
      {
        "name": "exitAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "tokenAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "to",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "deployShieldPortal",
    "inputs": [
      {
        "name": "note",
        "type": "tuple",
        "internalType": "struct CurvyTypes.Note",
        "components": [
          {
            "name": "ownerHash",
            "type": "uint256",
            "internalType": "uint256"
          },
          {
            "name": "token",
            "type": "uint256",
            "internalType": "uint256"
          },
          {
            "name": "amount",
            "type": "uint256",
            "internalType": "uint256"
          },
          {
            "name": "ephemeralKey",
            "type": "uint256[2]",
            "internalType": "uint256[2]"
          },
          {
            "name": "viewTag",
            "type": "uint16",
            "internalType": "uint16"
          }
        ]
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "deploySolanaExitBridgePortal",
    "inputs": [
      {
        "name": "bridgeData",
        "type": "bytes",
        "internalType": "bytes"
      },
      {
        "name": "amount",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "currency",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "exitAddress",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "gasFee",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "deploySolanaRecoveryExitPortal",
    "inputs": [
      {
        "name": "exitAddress",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "tokenAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "to",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "getEntryPortalAddress",
    "inputs": [
      {
        "name": "ownerHash",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getExitPortalAddress",
    "inputs": [
      {
        "name": "exitAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getRoleAdmin",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getSolanaExitPortalAddress",
    "inputs": [
      {
        "name": "exitAddress",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "grantRole",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "account",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "hasRole",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "account",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "owner",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "portalImpl",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "portalIsRegistered",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "renounceOwnership",
    "inputs": [],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "renounceRole",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "callerConfirmation",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "revokeRole",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "internalType": "bytes32"
      },
      {
        "name": "account",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "solanaPortalImpl",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "supportsInterface",
    "inputs": [
      {
        "name": "interfaceId",
        "type": "bytes4",
        "internalType": "bytes4"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "transferOwnership",
    "inputs": [
      {
        "name": "newOwner",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "updateConfig",
    "inputs": [
      {
        "name": "curvyVaultProxyAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "curvyAggregatorAlphaProxyAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "lifiDiamondAddress",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "nonpayable"
  },
  {
    "type": "event",
    "name": "ConfigUpdated",
    "inputs": [
      {
        "name": "curvyVaultProxyAddress",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      },
      {
        "name": "curvyAggregatorAlphaProxyAddress",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      },
      {
        "name": "lifiDiamondAddress",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "EntryBridgePortalDeployed",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "ownerHash",
        "type": "uint256",
        "indexed": true,
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "currency",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "ExitBridgePortalDeployed",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "exitAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "currency",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "OwnershipTransferred",
    "inputs": [
      {
        "name": "previousOwner",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "newOwner",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "RecoveryPortalDeployed",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "tokenAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "to",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "RoleAdminChanged",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      },
      {
        "name": "previousAdminRole",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      },
      {
        "name": "newAdminRole",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "RoleGranted",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      },
      {
        "name": "account",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "sender",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "RoleRevoked",
    "inputs": [
      {
        "name": "role",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      },
      {
        "name": "account",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "sender",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "ShieldPortalDeployed",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "ownerHash",
        "type": "uint256",
        "indexed": true,
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "SolanaExitBridgePortalDeployed",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "exitAddress",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      },
      {
        "name": "exitChainId",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      },
      {
        "name": "recovery",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "currency",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "SolanaRecoveryPortalDeployed",
    "inputs": [
      {
        "name": "portalAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "exitAddress",
        "type": "bytes32",
        "indexed": true,
        "internalType": "bytes32"
      },
      {
        "name": "tokenAddress",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "to",
        "type": "address",
        "indexed": false,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "error",
    "name": "AccessControlBadConfirmation",
    "inputs": []
  },
  {
    "type": "error",
    "name": "AccessControlUnauthorizedAccount",
    "inputs": [
      {
        "name": "account",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "neededRole",
        "type": "bytes32",
        "internalType": "bytes32"
      }
    ]
  },
  {
    "type": "error",
    "name": "AmountMismatch",
    "inputs": []
  },
  {
    "type": "error",
    "name": "DeploymentFailed",
    "inputs": []
  },
  {
    "type": "error",
    "name": "FailedDeployment",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InsufficientBalance",
    "inputs": [
      {
        "name": "balance",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "needed",
        "type": "uint256",
        "internalType": "uint256"
      }
    ]
  },
  {
    "type": "error",
    "name": "InvalidLiFiDestinationChain",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InvalidLiFiReceiver",
    "inputs": []
  },
  {
    "type": "error",
    "name": "OwnableInvalidOwner",
    "inputs": [
      {
        "name": "owner",
        "type": "address",
        "internalType": "address"
      }
    ]
  },
  {
    "type": "error",
    "name": "OwnableUnauthorizedAccount",
    "inputs": [
      {
        "name": "account",
        "type": "address",
        "internalType": "address"
      }
    ]
  },
  {
    "type": "error",
    "name": "UnsupportedBridging",
    "inputs": []
  },
  {
    "type": "error",
    "name": "UnsupportedShielding",
    "inputs": []
  }
]
```*/
#[allow(
    non_camel_case_types,
    non_snake_case,
    clippy::pub_underscore_fields,
    clippy::style,
    clippy::empty_structs_with_brackets
)]
pub mod PortalFactory {
    use super::*;
    use alloy::sol_types as alloy_sol_types;
    /// The creation / init bytecode of the contract.
    ///
    /// ```text
    ///0x7f63757276792d706f7274616c2d666163746f72792d73616c740000000000000060e052601960c05260f96040527f0c1d487ad400ae94b5e3f681571516a683db363b3743ef8072d065745915e37e60025534801561005c575f5ffd5b50604051613f12380380613f1283398101604081905261007b916102cf565b806001600160a01b0381166100a957604051631e4fbdf760e01b81525f600482015260240160405180910390fd5b6100b281610188565b506100d75f516020613ef25f395f51905f525f516020613ed25f395f51905f526101d7565b6100ee5f516020613ed25f395f51905f52806101d7565b6101055f516020613ed25f395f51905f5282610223565b5061011d5f516020613ef25f395f51905f5282610223565b5060405161012a906102b5565b604051809103905ff080158015610143573d5f5f3e3d5ffd5b506001600160a01b031660805260405161015c906102c2565b604051809103905ff080158015610175573d5f5f3e3d5ffd5b506001600160a01b031660a052506102fc565b5f80546001600160a01b038381166001600160a01b0319831681178455604051919092169283917f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e09190a35050565b5f828152600160208190526040808320909101805490849055905190918391839186917fbd79b86ffe0ab8e8776151514217cd7cacd52c909f66475c3af44e129f0b00ff9190a4505050565b5f8281526001602090815260408083206001600160a01b038516845290915281205460ff166102ac575f8381526001602081815260408084206001600160a01b0387168086529252808420805460ff19169093179092559051339286917f2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d9190a45060016102af565b505f5b92915050565b610f34806125e683390190565b6109b88061351a83390190565b5f602082840312156102df575f5ffd5b81516001600160a01b03811681146102f5575f5ffd5b9392505050565b60805160a0516122836103635f395f81816101c4015281816109c4015281816113b5015261153f01525f818161022e0152818161082b01528181610b4b01528181610c8801528181610cd401528181610f5401528181611145015261157701526122835ff3fe60806040526004361061017b575f3560e01c8063715018a6116100cd578063bc3488c611610087578063e16ca89511610062578063e16ca89514610474578063eb2347fd14610493578063f2fde38b146104ca578063f5b541a6146104e9575f5ffd5b8063bc3488c614610423578063d547741f14610436578063d80513b514610455575f5ffd5b8063715018a61461038f578063848c9f82146103a35780638da5cb5b146103b657806391d14854146103d2578063a217fddf146103f1578063b64b2a8a14610404575f5ffd5b80632f2ff15d116101385780634a3fba0e116101135780634a3fba0e146102ff57806353070b55146103325780635e8b95d21461035157806366e93b8c14610370575f5ffd5b80632f2ff15d146102a25780632f3819e6146102c157806336568abe146102e0575f5ffd5b806301ffc9a71461017f5780630c3148f5146101b357806311c9a94d146101fe5780631b7cac5f1461021d578063248a9ca3146102505780632a33cf2e1461028d575b5f5ffd5b34801561018a575f5ffd5b5061019e61019936600461199b565b610509565b60405190151581526020015b60405180910390f35b3480156101be575f5ffd5b506101e67f000000000000000000000000000000000000000000000000000000000000000081565b6040516001600160a01b0390911681526020016101aa565b348015610209575f5ffd5b5061019e6102183660046119d6565b61053f565b348015610228575f5ffd5b506101e67f000000000000000000000000000000000000000000000000000000000000000081565b34801561025b575f5ffd5b5061027f61026a366004611a1e565b5f908152600160208190526040909120015490565b6040519081526020016101aa565b6102a061029b366004611a7a565b61064f565b005b3480156102ad575f5ffd5b506102a06102bc366004611b0d565b610985565b3480156102cc575f5ffd5b506102a06102db366004611b3b565b6109b0565b3480156102eb575f5ffd5b506102a06102fa366004611b0d565b610afe565b34801561030a575f5ffd5b5061027f7fd565e3fc066df348a5cbc05a8d6323e00552838041cea2d84cc59876ba37735d81565b34801561033d575f5ffd5b506102a061034c366004611b96565b610b36565b34801561035c575f5ffd5b506101e661036b366004611b0d565b610c82565b34801561037b575f5ffd5b506102a061038a366004611be6565b610cbf565b34801561039a575f5ffd5b506102a0610e0c565b6102a06103b1366004611d4a565b610e1f565b3480156103c1575f5ffd5b505f546001600160a01b03166101e6565b3480156103dd575f5ffd5b5061019e6103ec366004611b0d565b6110ad565b3480156103fc575f5ffd5b5061027f5f81565b34801561040f575f5ffd5b506102a061041e366004611dcd565b6110d7565b6102a0610431366004611df8565b611299565b348015610441575f5ffd5b506102a0610450366004611b0d565b611514565b348015610460575f5ffd5b506101e661046f366004611e6a565b611539565b34801561047f575f5ffd5b506101e661048e366004611e95565b611571565b34801561049e575f5ffd5b5061019e6104ad366004611ec9565b6001600160a01b03165f9081526006602052604090205460ff1690565b3480156104d5575f5ffd5b506102a06104e4366004611ec9565b6115a2565b3480156104f4575f5ffd5b5061027f5f51602061222e5f395f51905f5281565b5f6001600160e01b03198216637965db0b60e01b148061053957506301ffc9a760e01b6001600160e01b03198316145b92915050565b5f7fd565e3fc066df348a5cbc05a8d6323e00552838041cea2d84cc59876ba37735d61056a816115e4565b6001600160a01b0385163b1561059657600380546001600160a01b0319166001600160a01b0387161790555b6001600160a01b0384163b156105c257600480546001600160a01b0319166001600160a01b0386161790555b6001600160a01b0383163b156105ee57600580546001600160a01b0319166001600160a01b0385161790555b600354600454600554604080516001600160a01b039485168152928416602084015292168183015290517fff8a97fda7728495ee3a5c551af3495e4ba23cdda2ac138491eff294902122ee9181900360600190a1600191505b509392505050565b5f51602061222e5f395f51905f52610666816115e4565b6005546001600160a01b031661068f5760405163437f3ac360e01b815260040160405180910390fd5b4684036107455760055460405163618c776d60e11b81525f916001600160a01b03169063c318eeda906106c8908d908d90600401611f0c565b60a060405180830381865afa1580156106e3573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906107079190611f2a565b9050856001600160a01b031681604001516001600160a01b03161461073f5760405163523660f760e01b815260040160405180910390fd5b50610816565b600554604051637f99d7af60e01b81525f916001600160a01b031690637f99d7af90610777908d908d90600401611f0c565b5f60405180830381865afa158015610791573d5f5f3e3d5ffd5b505050506040513d5f823e601f3d908101601f191682016040526107b8919081019061202e565b9050856001600160a01b03168160a001516001600160a01b0316146107f05760405163523660f760e01b815260040160405180910390fd5b848160e0015114610814576040516397c91b6960e01b815260040160405180910390fd5b505b5f6108235f8787876115ee565b90505f6108507f000000000000000000000000000000000000000000000000000000000000000083611645565b604051633b7e75bb60e11b81529091506001600160a01b038216906376fceb7690610885905f908b908b908b9060040161213d565b5f604051808303815f87803b15801561089c575f5ffd5b505af11580156108ae573d5f5f3e3d5ffd5b50505050806001600160a01b031663e3f5c5523460055f9054906101000a90046001600160a01b03168e8e8e8e8b6040518863ffffffff1660e01b81526004016108fd96959493929190612164565b5f604051808303818588803b158015610914575f5ffd5b505af1158015610926573d5f5f3e3d5ffd5b5050604080518a81526001600160a01b038d81166020830152808b1695508c81169450861692507fd32efcefa11997ebea13e4c365a2a92eefc7b965a74ba151dadb02a3cd500637910160405180910390a45050505050505050505050565b5f82815260016020819052604090912001546109a0816115e4565b6109aa8383611651565b50505050565b5f6109bc8686866116c7565b90505f6109e97f000000000000000000000000000000000000000000000000000000000000000083611645565b604051636315a14760e11b815260048101899052602481018890526001600160a01b0387811660448301529192509082169063c62b428e906064015f604051808303815f87803b158015610a3b575f5ffd5b505af1158015610a4d573d5f5f3e3d5ffd5b5050604051631922fddd60e21b81526001600160a01b03878116600483015286811660248301528416925063648bf77491506044015f604051808303815f87803b158015610a99575f5ffd5b505af1158015610aab573d5f5f3e3d5ffd5b50506040516001600160a01b03868116825280881693508a92508416907f2113859f5b6983a70726932ab3518e5bf454f731fdb16d96d8ac870a5b2322419060200160405180910390a450505050505050565b6001600160a01b0381163314610b275760405163334bd91960e11b815260040160405180910390fd5b610b318282611715565b505050565b5f610b43855f5f876115ee565b90505f610b707f000000000000000000000000000000000000000000000000000000000000000083611645565b604051633b7e75bb60e11b81529091506001600160a01b038216906376fceb7690610ba59089905f9081908b9060040161213d565b5f604051808303815f87803b158015610bbc575f5ffd5b505af1158015610bce573d5f5f3e3d5ffd5b5050604051631922fddd60e21b81526001600160a01b03878116600483015286811660248301528416925063648bf77491506044015f604051808303815f87803b158015610c1a575f5ffd5b505af1158015610c2c573d5f5f3e3d5ffd5b50505050826001600160a01b0316846001600160a01b0316826001600160a01b03167f248ad0b6173546f5b68ca1ea8493871fb220561b577d7d5fdf7bf404c1c1f87960405160405180910390a4505050505050565b5f610cb87f0000000000000000000000000000000000000000000000000000000000000000610cb3855f5f876115ee565b611780565b9392505050565b5f610ccc5f8787876115ee565b90505f610cf97f000000000000000000000000000000000000000000000000000000000000000083611645565b604051633b7e75bb60e11b81529091506001600160a01b038216906376fceb7690610d2e905f908b908b908b9060040161213d565b5f604051808303815f87803b158015610d45575f5ffd5b505af1158015610d57573d5f5f3e3d5ffd5b5050604051631922fddd60e21b81526001600160a01b03878116600483015286811660248301528416925063648bf77491506044015f604051808303815f87803b158015610da3575f5ffd5b505af1158015610db5573d5f5f3e3d5ffd5b50505050826001600160a01b0316846001600160a01b0316826001600160a01b03167f248ad0b6173546f5b68ca1ea8493871fb220561b577d7d5fdf7bf404c1c1f87960405160405180910390a450505050505050565b610e146117e8565b610e1d5f611814565b565b5f51602061222e5f395f51905f52610e36816115e4565b6005546001600160a01b0316610e5f5760405163437f3ac360e01b815260040160405180910390fd5b600554604051637f99d7af60e01b81525f916001600160a01b031690637f99d7af90610e91908b908b90600401611f0c565b5f60405180830381865afa158015610eab573d5f5f3e3d5ffd5b505050506040513d5f823e601f3d908101601f19168201604052610ed2919081019061202e565b9050610ee1865f015185610c82565b6001600160a01b03168160a001516001600160a01b031614610f165760405163523660f760e01b815260040160405180910390fd5b61a4b18160e0015114610f3c576040516397c91b6960e01b815260040160405180910390fd5b5f610f4c875f01515f5f886115ee565b90505f610f797f000000000000000000000000000000000000000000000000000000000000000083611645565b8851604051633b7e75bb60e11b81529192506001600160a01b038316916376fceb7691610fae915f9081908c9060040161213d565b5f604051808303815f87803b158015610fc5575f5ffd5b505af1158015610fd7573d5f5f3e3d5ffd5b50505050806001600160a01b031663e3f5c5523460055f9054906101000a90046001600160a01b03168d8d8d604001518d8c6040518863ffffffff1660e01b815260040161102a96959493929190612164565b5f604051808303818588803b158015611041575f5ffd5b505af1158015611053573d5f5f3e3d5ffd5b50508a516040516001600160a01b038c81168252808c16955091935090851691507f0d580c10960bd9b30908da4edf8e5f5ba6b1fbaf2c32d28be175a92d4b9896f29060200160405180910390a450505050505050505050565b5f9182526001602090815260408084206001600160a01b0393909316845291905290205460ff1690565b5f51602061222e5f395f51905f526110ee816115e4565b6003546001600160a01b0316158061110f57506004546001600160a01b0316155b1561112d576040516389da714f60e01b815260040160405180910390fd5b5f61113d845f01515f5f866115ee565b90505f61116a7f000000000000000000000000000000000000000000000000000000000000000083611645565b8551604051633b7e75bb60e11b81529192506001600160a01b038316916376fceb769161119f915f9081908a9060040161213d565b5f604051808303815f87803b1580156111b6575f5ffd5b505af11580156111c8573d5f5f3e3d5ffd5b5050506001600160a01b038083165f8181526006602052604090819020805460ff19166001179055600480546003549251630cce2a2760e11b815293955063199c544e94611221948c94928216939290911691016121af565b5f604051808303815f87803b158015611238575f5ffd5b505af115801561124a573d5f5f3e3d5ffd5b50505050836001600160a01b0316855f0151826001600160a01b03167f7153fc1ba1d8a6784e0a3aa1113e319ebf6702ccbdcc83a126a5d23b2671b07460405160405180910390a45050505050565b5f51602061222e5f395f51905f526112b0816115e4565b6005546001600160a01b03166112d95760405163437f3ac360e01b815260040160405180910390fd5b600554604051637f99d7af60e01b81525f916001600160a01b031690637f99d7af9061130b908d908d90600401611f0c565b5f60405180830381865afa158015611325573d5f5f3e3d5ffd5b505050506040513d5f823e601f3d908101601f1916820160405261134c919081019061202e565b9050848160e0015114611372576040516397c91b6960e01b815260040160405180910390fd5b856113838b8b846101000151611863565b146113a15760405163523660f760e01b815260040160405180910390fd5b5f6113ad8787876116c7565b90505f6113da7f000000000000000000000000000000000000000000000000000000000000000083611645565b604051636315a14760e11b8152600481018a9052602481018990526001600160a01b0388811660448301529192509082169063c62b428e906064015f604051808303815f87803b15801561142c575f5ffd5b505af115801561143e573d5f5f3e3d5ffd5b50505050806001600160a01b031663e3f5c5523460055f9054906101000a90046001600160a01b03168f8f8f8f8c6040518863ffffffff1660e01b815260040161148d96959493929190612164565b5f604051808303818588803b1580156114a4575f5ffd5b505af11580156114b6573d5f5f3e3d5ffd5b5050604080518b81526001600160a01b038e81166020830152808c1695508d9450861692507f2159a9beac3a6bdf488be58a40cb4df3c7649082933a290f1d86a43ebba2d213910160405180910390a4505050505050505050505050565b5f828152600160208190526040909120015461152f816115e4565b6109aa8383611715565b5f6115697f0000000000000000000000000000000000000000000000000000000000000000610cb38686866116c7565b949350505050565b5f6115697f0000000000000000000000000000000000000000000000000000000000000000610cb35f8787876115ee565b6115aa6117e8565b6001600160a01b0381166115d857604051631e4fbdf760e01b81525f60048201526024015b60405180910390fd5b6115e181611814565b50565b6115e181336118c9565b60025460408051602081019290925281018590526001600160a01b03808516606083015260808201849052821660a08201525f9060c001604051602081830303815290604052805190602001209050949350505050565b5f610cb883835f611906565b5f61165c83836110ad565b6116c0575f8381526001602081815260408084206001600160a01b0387168086529252808420805460ff19169093179092559051339286917f2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d9190a4506001610539565b505f610539565b6002546040805160208101929092528101849052606081018390526001600160a01b03821660808201525f9060a0016040516020818303038152906040528051906020012090509392505050565b5f61172083836110ad565b156116c0575f8381526001602090815260408083206001600160a01b0386168085529252808320805460ff1916905551339286917ff6391f5c32d9c69d2a47ea670b442974b53935d1edc7fd64eb21e047a839171b9190a4506001610539565b6040513060388201526f5af43d82803e903d91602b57fd5bf3ff602482015260148101839052733d602d80600a3d3981f3363d3d373d3d3d363d738152605881018290526037600c820120607882015260556043909101205f906001600160a01b0316610cb8565b5f546001600160a01b03163314610e1d5760405163118cdaa760e01b81523360048201526024016115cf565b5f80546001600160a01b038381166001600160a01b0319831681178455604051919092169283917f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e09190a35050565b5f5f84848080601f0160208091040260200160405190810160405280939291908181526020018383808284375f9201919091525092935050841591506118b790505760648101518101602401519150610647565b60448101510160240151949350505050565b6118d382826110ad565b6119025760405163e2517d3f60e01b81526001600160a01b0382166004820152602481018390526044016115cf565b5050565b5f814710156119315760405163cf47918160e01b8152476004820152602481018390526044016115cf565b763d602d80600a3d3981f3363d3d373d3d3d363d730000008460601b60e81c175f526e5af43d82803e903d91602b57fd5bf38460781b17602052826037600984f590506001600160a01b038116610cb85760405163b06ebf3d60e01b815260040160405180910390fd5b5f602082840312156119ab575f5ffd5b81356001600160e01b031981168114610cb8575f5ffd5b6001600160a01b03811681146115e1575f5ffd5b5f5f5f606084860312156119e8575f5ffd5b83356119f3816119c2565b92506020840135611a03816119c2565b91506040840135611a13816119c2565b809150509250925092565b5f60208284031215611a2e575f5ffd5b5035919050565b5f5f83601f840112611a45575f5ffd5b50813567ffffffffffffffff811115611a5c575f5ffd5b602083019150836020828501011115611a73575f5ffd5b9250929050565b5f5f5f5f5f5f5f5f60e0898b031215611a91575f5ffd5b883567ffffffffffffffff811115611aa7575f5ffd5b611ab38b828c01611a35565b909950975050602089013595506040890135611ace816119c2565b94506060890135611ade816119c2565b93506080890135925060a0890135611af5816119c2565b979a969950949793969295919450919260c001359150565b5f5f60408385031215611b1e575f5ffd5b823591506020830135611b30816119c2565b809150509250929050565b5f5f5f5f5f60a08688031215611b4f575f5ffd5b85359450602086013593506040860135611b68816119c2565b92506060860135611b78816119c2565b91506080860135611b88816119c2565b809150509295509295909350565b5f5f5f5f60808587031215611ba9575f5ffd5b843593506020850135611bbb816119c2565b92506040850135611bcb816119c2565b91506060850135611bdb816119c2565b939692955090935050565b5f5f5f5f5f60a08688031215611bfa575f5ffd5b8535611c05816119c2565b9450602086013593506040860135611b68816119c2565b634e487b7160e01b5f52604160045260245ffd5b60405160a0810167ffffffffffffffff81118282101715611c5357611c53611c1c565b60405290565b6040805190810167ffffffffffffffff81118282101715611c5357611c53611c1c565b604051610140810167ffffffffffffffff81118282101715611c5357611c53611c1c565b803561ffff81168114611cb1575f5ffd5b919050565b5f60c08284031215611cc6575f5ffd5b611cce611c30565b8235815260208084013590820152604080840135908201529050607f82018313611cf6575f5ffd5b611cfe611c59565b8060a0840185811115611d0f575f5ffd5b606085015b81811015611d2c578035845260209384019301611d14565b50816060850152611d3c81611ca0565b608085015250505092915050565b5f5f5f5f5f5f6101408789031215611d60575f5ffd5b863567ffffffffffffffff811115611d76575f5ffd5b611d8289828a01611a35565b9097509550611d9690508860208901611cb6565b935060e0870135611da6816119c2565b9250610100870135611db7816119c2565b9598949750929591949361012090920135925050565b5f5f60e08385031215611dde575f5ffd5b611de88484611cb6565b915060c0830135611b30816119c2565b5f5f5f5f5f5f5f5f60e0898b031215611e0f575f5ffd5b883567ffffffffffffffff811115611e25575f5ffd5b611e318b828c01611a35565b909950975050602089013595506040890135611e4c816119c2565b9450606089013593506080890135925060a0890135611af5816119c2565b5f5f5f60608486031215611e7c575f5ffd5b83359250602084013591506040840135611a13816119c2565b5f5f5f60608486031215611ea7575f5ffd5b8335611eb2816119c2565b9250602084013591506040840135611a13816119c2565b5f60208284031215611ed9575f5ffd5b8135610cb8816119c2565b81835281816020850137505f828201602090810191909152601f909101601f19169091010190565b602081525f611569602083018486611ee4565b8051611cb1816119c2565b5f60a0828403128015611f3b575f5ffd5b50611f44611c30565b8251611f4f816119c2565b8152602083810151908201526040830151611f69816119c2565b60408201526060830151611f7c816119c2565b60608201526080928301519281019290925250919050565b5f82601f830112611fa3575f5ffd5b815167ffffffffffffffff811115611fbd57611fbd611c1c565b604051601f8201601f19908116603f0116810167ffffffffffffffff81118282101715611fec57611fec611c1c565b604052818152838201602001851015612003575f5ffd5b8160208501602083015e5f918101602001919091529392505050565b80518015158114611cb1575f5ffd5b5f6020828403121561203e575f5ffd5b815167ffffffffffffffff811115612054575f5ffd5b82016101408185031215612066575f5ffd5b61206e611c7c565b81518152602082015167ffffffffffffffff81111561208b575f5ffd5b61209786828501611f94565b602083015250604082015167ffffffffffffffff8111156120b6575f5ffd5b6120c286828501611f94565b6040830152506120d460608301611f1f565b60608201526120e560808301611f1f565b60808201526120f660a08301611f1f565b60a082015260c0828101519082015260e0808301519082015261211c610100830161201f565b61010082015261212f610120830161201f565b610120820152949350505050565b9384526001600160a01b039283166020850152604084019190915216606082015260800190565b6001600160a01b038716815260a0602082018190525f906121889083018789611ee4565b6040830195909552506001600160a01b039290921660608301526080909101529392505050565b5f610100820190508451825260208501516020830152604085015160408301526060850151606083015f5b60028110156121f95782518252602092830192909101906001016121da565b505050608085015161ffff1660a08301526001600160a01b03841660c08301526001600160a01b03831660e083015261156956fe97667070c54ef182b0f5858b034beac1b6f3089aa2d3188bb1e8929f4fa9b929a26469706673582212201492fef915baf9e19fe3e53e36603890943cf588d0d779f0c71db4183307576a64736f6c634300081c00336080604052348015600e575f5ffd5b506005805460ff60a81b1916600160a81b179055610f058061002f5f395ff3fe60806040526004361061006e575f3560e01c8063648bf7741161004c578063648bf774146100ed57806376fceb761461010c578063ddceafa91461012b578063e3f5c5521461014a575f5ffd5b8063199c544e146100725780633fb07027146100935780635dc24ee3146100ce575b5f5ffd5b34801561007d575f5ffd5b5061009161008c366004610c21565b61015d565b005b34801561009e575f5ffd5b506004546100b2906001600160a01b031681565b6040516001600160a01b03909116815260200160405180910390f35b3480156100d9575f5ffd5b506003546100b2906001600160a01b031681565b3480156100f8575f5ffd5b50610091610107366004610ce6565b6103a2565b348015610117575f5ffd5b50610091610126366004610d1d565b6105b9565b348015610136575f5ffd5b506005546100b2906001600160a01b031681565b610091610158366004610d64565b61068e565b600554600160a01b900460ff16156101b65760405162461bcd60e51b815260206004820152601760248201527614da5b99db19555cd94e88105b1c9958591e481d5cd959604a1b60448201526064015b60405180910390fd5b5f548351146101d75760405162cb7dff60e81b815260040160405180910390fd5b600380546001600160a01b038481166001600160a01b0319928316179092556004805492841692909116821781556020850151604051630cf99be760e31b8152918201525f91906367ccdf3890602401602060405180830381865afa158015610242573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906102669190610e08565b90506001600160a01b0381161580159061029d57506001600160a01b03811673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b156103245760035460408501516102c2916001600160a01b03848116929116906109df565b60035460405163ba48d11760e01b81526001600160a01b039091169063ba48d117906102f2908790600401610e2a565b5f604051808303815f87803b158015610309575f5ffd5b505af115801561031b573d5f5f3e3d5ffd5b50505050610389565b600354604080860151905163ba48d11760e01b81526001600160a01b039092169163ba48d117919061035a908890600401610e2a565b5f604051808303818588803b158015610371575f5ffd5b505af1158015610383573d5f5f3e3d5ffd5b50505050505b50506005805460ff60a01b1916600160a01b1790555050565b6005546001600160a01b031632146103f45760405162461bcd60e51b8152602060048201526015602482015274506f7274616c3a204f6e6c79207265636f7665727960581b60448201526064016101ad565b6001600160a01b03811661041b5760405163530a10d160e11b815260040160405180910390fd5b6001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee148061044d57506001600160a01b038216155b1561051957475f81900361047457604051636165515360e11b815260040160405180910390fd5b5f826001600160a01b0316826040515f6040518083038185875af1925050503d805f81146104bd576040519150601f19603f3d011682016040523d82523d5f602084013e6104c2565b606091505b50509050806105135760405162461bcd60e51b815260206004820152601b60248201527f506f7274616c3a20455448207472616e73666572206661696c6564000000000060448201526064016101ad565b50505050565b6040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa15801561055f573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906105839190610e8a565b9050805f036105a557604051636165515360e11b815260040160405180910390fd5b6105136001600160a01b0383168483610a9c565b600554600160a81b900460ff16156105e35760405162dc149f60e41b815260040160405180910390fd5b6005805460ff60a81b1916600160a81b1790556001600160a01b03811661061d5760405163530a10d160e11b815260040160405180910390fd5b83156001600160a01b038416151480610637575083158215145b1561065557604051631d4deb8b60e31b815260040160405180910390fd5b5f93909355600180546001600160a01b039384166001600160a01b03199182161790915560029190915560058054929093169116179055565b600554600160a01b900460ff16156106e25760405162461bcd60e51b815260206004820152601760248201527614da5b99db19555cd94e88105b1c9958591e481d5cd959604a1b60448201526064016101ad565b6001600160a01b0382161580159061071757506001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b15610883576040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa158015610762573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906107869190610e8a565b9050848110156107a957604051637bdd7ae760e01b815260040160405180910390fd5b6107bd6001600160a01b03831689876109df565b5f5f896001600160a01b0316348a8a6040516107da929190610ea1565b5f6040518083038185875af1925050503d805f8114610814576040519150601f19603f3d011682016040523d82523d5f602084013e610819565b606091505b5090925090506108336001600160a01b0385168b5f6109df565b841561084d5761084d6001600160a01b0385163287610a9c565b8161087a5780511561086157805181602001fd5b604051631bb7daad60e11b815260040160405180910390fd5b505050506109c4565b47838110156108a557604051637bdd7ae760e01b815260040160405180910390fd5b5f806001600160a01b0389166108bb3488610eb0565b89896040516108cb929190610ea1565b5f6040518083038185875af1925050503d805f8114610905576040519150601f19603f3d011682016040523d82523d5f602084013e61090a565b606091505b5091509150816109235780511561086157805181602001fd5b83156109c0576040515f90329086908381818185875af1925050503d805f8114610968576040519150601f19603f3d011682016040523d82523d5f602084013e61096d565b606091505b50509050806109be5760405162461bcd60e51b815260206004820152601c60248201527f506f7274616c3a207265696d62757273656d656e74206661696c65640000000060448201526064016101ad565b505b5050505b50506005805460ff60a01b1916600160a01b17905550505050565b604080516001600160a01b038416602482015260448082018490528251808303909101815260649091019091526020810180516001600160e01b031663095ea7b360e01b179052610a308482610ad2565b610513576040516001600160a01b0384811660248301525f6044830152610a9291869182169063095ea7b3906064015b604051602081830303815290604052915060e01b6020820180516001600160e01b038381831617835250505050610b1d565b6105138482610b1d565b6040516001600160a01b03838116602483015260448201839052610acd91859182169063a9059cbb90606401610a60565b505050565b5f5f5f5f60205f8651602088015f8a5af192503d91505f519050828015610b1157508115610b035780600114610b11565b5f866001600160a01b03163b115b93505050505b92915050565b5f5f60205f8451602086015f885af180610b3c576040513d5f823e3d81fd5b50505f513d91508115610b53578060011415610b60565b6001600160a01b0384163b155b1561051357604051635274afe760e01b81526001600160a01b03851660048201526024016101ad565b634e487b7160e01b5f52604160045260245ffd5b60405160a0810167ffffffffffffffff81118282101715610bc057610bc0610b89565b60405290565b6040805190810167ffffffffffffffff81118282101715610bc057610bc0610b89565b803561ffff81168114610bfa575f5ffd5b919050565b6001600160a01b0381168114610c13575f5ffd5b50565b8035610bfa81610bff565b5f5f5f838503610100811215610c35575f5ffd5b60c0811215610c42575f5ffd5b50610c4b610b9d565b843581526020808601359082015260408086013590820152607f85018613610c71575f5ffd5b610c79610bc6565b8060a0870188811115610c8a575f5ffd5b606088015b81811015610ca7578035845260209384019301610c8f565b50816060850152610cb781610be9565b608085015250505080935050610ccf60c08501610c16565b9150610cdd60e08501610c16565b90509250925092565b5f5f60408385031215610cf7575f5ffd5b8235610d0281610bff565b91506020830135610d1281610bff565b809150509250929050565b5f5f5f5f60808587031215610d30575f5ffd5b843593506020850135610d4281610bff565b9250604085013591506060850135610d5981610bff565b939692955090935050565b5f5f5f5f5f5f60a08789031215610d79575f5ffd5b8635610d8481610bff565b9550602087013567ffffffffffffffff811115610d9f575f5ffd5b8701601f81018913610daf575f5ffd5b803567ffffffffffffffff811115610dc5575f5ffd5b896020828401011115610dd6575f5ffd5b6020919091019550935060408701359250610df360608801610c16565b95989497509295919493608090920135925050565b5f60208284031215610e18575f5ffd5b8151610e2381610bff565b9392505050565b5f60c0820190508251825260208301516020830152604083015160408301526060830151606083015f5b6002811015610e73578251825260209283019290910190600101610e54565b50505061ffff60808401511660a083015292915050565b5f60208284031215610e9a575f5ffd5b5051919050565b818382375f9101908152919050565b80820180821115610b1757634e487b7160e01b5f52601160045260245ffdfea26469706673582212202c359c5676bff8bb9be336fb2e0527b4337d8c884e4c9e03679a546f7bda8bea64736f6c634300081c00336080604052348015600e575f5ffd5b506002805460ff60a81b1916600160a81b1790556109898061002f5f395ff3fe60806040526004361061003e575f3560e01c8063648bf77414610042578063c62b428e14610063578063ddceafa914610082578063e3f5c552146100bd575b5f5ffd5b34801561004d575f5ffd5b5061006161005c366004610828565b6100d0565b005b34801561006e575f5ffd5b5061006161007d366004610859565b610302565b34801561008d575f5ffd5b506002546100a1906001600160a01b031681565b6040516001600160a01b03909116815260200160405180910390f35b6100616100cb36600461088b565b6103b8565b6002546001600160a01b0316321461012f5760405162461bcd60e51b815260206004820152601f60248201527f506f7274616c536f6c616e61457869743a204f6e6c79207265636f766572790060448201526064015b60405180910390fd5b6001600160a01b0381166101565760405163530a10d160e11b815260040160405180910390fd5b6001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee148061018857506001600160a01b038216155b1561026257475f8190036101af57604051636165515360e11b815260040160405180910390fd5b5f826001600160a01b0316826040515f6040518083038185875af1925050503d805f81146101f8576040519150601f19603f3d011682016040523d82523d5f602084013e6101fd565b606091505b505090508061025c5760405162461bcd60e51b815260206004820152602560248201527f506f7274616c536f6c616e61457869743a20455448207472616e736665722066604482015264185a5b195960da1b6064820152608401610126565b50505050565b6040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa1580156102a8573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906102cc919061092d565b9050805f036102ee57604051636165515360e11b815260040160405180910390fd5b61025c6001600160a01b0383168483610669565b600254600160a81b900460ff161561032c5760405162dc149f60e41b815260040160405180910390fd5b6002805460ff60a81b1916600160a81b1790556001600160a01b0381166103665760405163530a10d160e11b815260040160405180910390fd5b821580610371575081155b1561038f57604051637fa466f960e01b815260040160405180910390fd5b5f92909255600155600280546001600160a01b0319166001600160a01b03909216919091179055565b600254600160a01b900460ff16156104125760405162461bcd60e51b815260206004820152601760248201527f53696e676c655573653a20416c726561647920757365640000000000000000006044820152606401610126565b6001600160a01b0382161580159061044757506001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b156105b3576040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa158015610492573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906104b6919061092d565b9050848110156104d957604051637bdd7ae760e01b815260040160405180910390fd5b6104ed6001600160a01b03831689876106cd565b5f5f896001600160a01b0316348a8a60405161050a929190610944565b5f6040518083038185875af1925050503d805f8114610544576040519150601f19603f3d011682016040523d82523d5f602084013e610549565b606091505b5090925090506105636001600160a01b0385168b5f6106cd565b841561057d5761057d6001600160a01b0385163387610669565b816105aa5780511561059157805181602001fd5b604051631bb7daad60e11b815260040160405180910390fd5b5050505061064e565b47838110156105d557604051637bdd7ae760e01b815260040160405180910390fd5b5f5f886001600160a01b03168689896040516105f2929190610944565b5f6040518083038185875af1925050503d805f811461062c576040519150601f19603f3d011682016040523d82523d5f602084013e610631565b606091505b50915091508161064a5780511561059157805181602001fd5b5050505b50506002805460ff60a01b1916600160a01b17905550505050565b6040516001600160a01b038381166024830152604482018390526106c891859182169063a9059cbb906064015b604051602081830303815290604052915060e01b6020820180516001600160e01b038381831617835250505050610758565b505050565b604080516001600160a01b038416602482015260448082018490528251808303909101815260649091019091526020810180516001600160e01b031663095ea7b360e01b17905261071e84826107c4565b61025c576040516001600160a01b0384811660248301525f604483015261075291869182169063095ea7b390606401610696565b61025c84825b5f5f60205f8451602086015f885af180610777576040513d5f823e3d81fd5b50505f513d9150811561078e57806001141561079b565b6001600160a01b0384163b155b1561025c57604051635274afe760e01b81526001600160a01b0385166004820152602401610126565b5f5f5f5f60205f8651602088015f8a5af192503d91505f519050828015610803575081156107f55780600114610803565b5f866001600160a01b03163b115b9695505050505050565b80356001600160a01b0381168114610823575f5ffd5b919050565b5f5f60408385031215610839575f5ffd5b6108428361080d565b91506108506020840161080d565b90509250929050565b5f5f5f6060848603121561086b575f5ffd5b83359250602084013591506108826040850161080d565b90509250925092565b5f5f5f5f5f5f60a087890312156108a0575f5ffd5b6108a98761080d565b9550602087013567ffffffffffffffff8111156108c4575f5ffd5b8701601f810189136108d4575f5ffd5b803567ffffffffffffffff8111156108ea575f5ffd5b8960208284010111156108fb575f5ffd5b60209190910195509350604087013592506109186060880161080d565b95989497509295919493608090920135925050565b5f6020828403121561093d575f5ffd5b5051919050565b818382375f910190815291905056fea2646970667358221220ce053c999625124d592a46e9d9ef4d9c25f366a1612ac07de554cd0d5d270fd164736f6c634300081c0033d565e3fc066df348a5cbc05a8d6323e00552838041cea2d84cc59876ba37735d97667070c54ef182b0f5858b034beac1b6f3089aa2d3188bb1e8929f4fa9b929
    /// ```
    #[rustfmt::skip]
    #[allow(clippy::all)]
    pub static BYTECODE: alloy_sol_types::private::Bytes = alloy_sol_types::private::Bytes::from_static(
        b"\x7Fcurvy-portal-factory-salt\0\0\0\0\0\0\0`\xE0R`\x19`\xC0R`\xF9`@R\x7F\x0C\x1DHz\xD4\0\xAE\x94\xB5\xE3\xF6\x81W\x15\x16\xA6\x83\xDB6;7C\xEF\x80r\xD0etY\x15\xE3~`\x02U4\x80\x15a\0\\W__\xFD[P`@Qa?\x128\x03\x80a?\x12\x839\x81\x01`@\x81\x90Ra\0{\x91a\x02\xCFV[\x80`\x01`\x01`\xA0\x1B\x03\x81\x16a\0\xA9W`@Qc\x1EO\xBD\xF7`\xE0\x1B\x81R_`\x04\x82\x01R`$\x01`@Q\x80\x91\x03\x90\xFD[a\0\xB2\x81a\x01\x88V[Pa\0\xD7_Q` a>\xF2_9_Q\x90_R_Q` a>\xD2_9_Q\x90_Ra\x01\xD7V[a\0\xEE_Q` a>\xD2_9_Q\x90_R\x80a\x01\xD7V[a\x01\x05_Q` a>\xD2_9_Q\x90_R\x82a\x02#V[Pa\x01\x1D_Q` a>\xF2_9_Q\x90_R\x82a\x02#V[P`@Qa\x01*\x90a\x02\xB5V[`@Q\x80\x91\x03\x90_\xF0\x80\x15\x80\x15a\x01CW=__>=_\xFD[P`\x01`\x01`\xA0\x1B\x03\x16`\x80R`@Qa\x01\\\x90a\x02\xC2V[`@Q\x80\x91\x03\x90_\xF0\x80\x15\x80\x15a\x01uW=__>=_\xFD[P`\x01`\x01`\xA0\x1B\x03\x16`\xA0RPa\x02\xFCV[_\x80T`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`\x01`\x01`\xA0\x1B\x03\x19\x83\x16\x81\x17\x84U`@Q\x91\x90\x92\x16\x92\x83\x91\x7F\x8B\xE0\x07\x9CS\x16Y\x14\x13D\xCD\x1F\xD0\xA4\xF2\x84\x19I\x7F\x97\"\xA3\xDA\xAF\xE3\xB4\x18okdW\xE0\x91\x90\xA3PPV[_\x82\x81R`\x01` \x81\x90R`@\x80\x83 \x90\x91\x01\x80T\x90\x84\x90U\x90Q\x90\x91\x83\x91\x83\x91\x86\x91\x7F\xBDy\xB8o\xFE\n\xB8\xE8waQQB\x17\xCD|\xAC\xD5,\x90\x9FfG\\:\xF4N\x12\x9F\x0B\0\xFF\x91\x90\xA4PPPV[_\x82\x81R`\x01` \x90\x81R`@\x80\x83 `\x01`\x01`\xA0\x1B\x03\x85\x16\x84R\x90\x91R\x81 T`\xFF\x16a\x02\xACW_\x83\x81R`\x01` \x81\x81R`@\x80\x84 `\x01`\x01`\xA0\x1B\x03\x87\x16\x80\x86R\x92R\x80\x84 \x80T`\xFF\x19\x16\x90\x93\x17\x90\x92U\x90Q3\x92\x86\x91\x7F/\x87\x88\x11~~\xFF\x1D\x82\xE9&\xECyI\x01\xD1|x\x02JP'\t@0E@\xA73eo\r\x91\x90\xA4P`\x01a\x02\xAFV[P_[\x92\x91PPV[a\x0F4\x80a%\xE6\x839\x01\x90V[a\t\xB8\x80a5\x1A\x839\x01\x90V[_` \x82\x84\x03\x12\x15a\x02\xDFW__\xFD[\x81Q`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x02\xF5W__\xFD[\x93\x92PPPV[`\x80Q`\xA0Qa\"\x83a\x03c_9_\x81\x81a\x01\xC4\x01R\x81\x81a\t\xC4\x01R\x81\x81a\x13\xB5\x01Ra\x15?\x01R_\x81\x81a\x02.\x01R\x81\x81a\x08+\x01R\x81\x81a\x0BK\x01R\x81\x81a\x0C\x88\x01R\x81\x81a\x0C\xD4\x01R\x81\x81a\x0FT\x01R\x81\x81a\x11E\x01Ra\x15w\x01Ra\"\x83_\xF3\xFE`\x80`@R`\x046\x10a\x01{W_5`\xE0\x1C\x80cqP\x18\xA6\x11a\0\xCDW\x80c\xBC4\x88\xC6\x11a\0\x87W\x80c\xE1l\xA8\x95\x11a\0bW\x80c\xE1l\xA8\x95\x14a\x04tW\x80c\xEB#G\xFD\x14a\x04\x93W\x80c\xF2\xFD\xE3\x8B\x14a\x04\xCAW\x80c\xF5\xB5A\xA6\x14a\x04\xE9W__\xFD[\x80c\xBC4\x88\xC6\x14a\x04#W\x80c\xD5Gt\x1F\x14a\x046W\x80c\xD8\x05\x13\xB5\x14a\x04UW__\xFD[\x80cqP\x18\xA6\x14a\x03\x8FW\x80c\x84\x8C\x9F\x82\x14a\x03\xA3W\x80c\x8D\xA5\xCB[\x14a\x03\xB6W\x80c\x91\xD1HT\x14a\x03\xD2W\x80c\xA2\x17\xFD\xDF\x14a\x03\xF1W\x80c\xB6K*\x8A\x14a\x04\x04W__\xFD[\x80c//\xF1]\x11a\x018W\x80cJ?\xBA\x0E\x11a\x01\x13W\x80cJ?\xBA\x0E\x14a\x02\xFFW\x80cS\x07\x0BU\x14a\x032W\x80c^\x8B\x95\xD2\x14a\x03QW\x80cf\xE9;\x8C\x14a\x03pW__\xFD[\x80c//\xF1]\x14a\x02\xA2W\x80c/8\x19\xE6\x14a\x02\xC1W\x80c6V\x8A\xBE\x14a\x02\xE0W__\xFD[\x80c\x01\xFF\xC9\xA7\x14a\x01\x7FW\x80c\x0C1H\xF5\x14a\x01\xB3W\x80c\x11\xC9\xA9M\x14a\x01\xFEW\x80c\x1B|\xAC_\x14a\x02\x1DW\x80c$\x8A\x9C\xA3\x14a\x02PW\x80c*3\xCF.\x14a\x02\x8DW[__\xFD[4\x80\x15a\x01\x8AW__\xFD[Pa\x01\x9Ea\x01\x996`\x04a\x19\x9BV[a\x05\tV[`@Q\x90\x15\x15\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[4\x80\x15a\x01\xBEW__\xFD[Pa\x01\xE6\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81V[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01a\x01\xAAV[4\x80\x15a\x02\tW__\xFD[Pa\x01\x9Ea\x02\x186`\x04a\x19\xD6V[a\x05?V[4\x80\x15a\x02(W__\xFD[Pa\x01\xE6\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81V[4\x80\x15a\x02[W__\xFD[Pa\x02\x7Fa\x02j6`\x04a\x1A\x1EV[_\x90\x81R`\x01` \x81\x90R`@\x90\x91 \x01T\x90V[`@Q\x90\x81R` \x01a\x01\xAAV[a\x02\xA0a\x02\x9B6`\x04a\x1AzV[a\x06OV[\0[4\x80\x15a\x02\xADW__\xFD[Pa\x02\xA0a\x02\xBC6`\x04a\x1B\rV[a\t\x85V[4\x80\x15a\x02\xCCW__\xFD[Pa\x02\xA0a\x02\xDB6`\x04a\x1B;V[a\t\xB0V[4\x80\x15a\x02\xEBW__\xFD[Pa\x02\xA0a\x02\xFA6`\x04a\x1B\rV[a\n\xFEV[4\x80\x15a\x03\nW__\xFD[Pa\x02\x7F\x7F\xD5e\xE3\xFC\x06m\xF3H\xA5\xCB\xC0Z\x8Dc#\xE0\x05R\x83\x80A\xCE\xA2\xD8L\xC5\x98v\xBA7s]\x81V[4\x80\x15a\x03=W__\xFD[Pa\x02\xA0a\x03L6`\x04a\x1B\x96V[a\x0B6V[4\x80\x15a\x03\\W__\xFD[Pa\x01\xE6a\x03k6`\x04a\x1B\rV[a\x0C\x82V[4\x80\x15a\x03{W__\xFD[Pa\x02\xA0a\x03\x8A6`\x04a\x1B\xE6V[a\x0C\xBFV[4\x80\x15a\x03\x9AW__\xFD[Pa\x02\xA0a\x0E\x0CV[a\x02\xA0a\x03\xB16`\x04a\x1DJV[a\x0E\x1FV[4\x80\x15a\x03\xC1W__\xFD[P_T`\x01`\x01`\xA0\x1B\x03\x16a\x01\xE6V[4\x80\x15a\x03\xDDW__\xFD[Pa\x01\x9Ea\x03\xEC6`\x04a\x1B\rV[a\x10\xADV[4\x80\x15a\x03\xFCW__\xFD[Pa\x02\x7F_\x81V[4\x80\x15a\x04\x0FW__\xFD[Pa\x02\xA0a\x04\x1E6`\x04a\x1D\xCDV[a\x10\xD7V[a\x02\xA0a\x0416`\x04a\x1D\xF8V[a\x12\x99V[4\x80\x15a\x04AW__\xFD[Pa\x02\xA0a\x04P6`\x04a\x1B\rV[a\x15\x14V[4\x80\x15a\x04`W__\xFD[Pa\x01\xE6a\x04o6`\x04a\x1EjV[a\x159V[4\x80\x15a\x04\x7FW__\xFD[Pa\x01\xE6a\x04\x8E6`\x04a\x1E\x95V[a\x15qV[4\x80\x15a\x04\x9EW__\xFD[Pa\x01\x9Ea\x04\xAD6`\x04a\x1E\xC9V[`\x01`\x01`\xA0\x1B\x03\x16_\x90\x81R`\x06` R`@\x90 T`\xFF\x16\x90V[4\x80\x15a\x04\xD5W__\xFD[Pa\x02\xA0a\x04\xE46`\x04a\x1E\xC9V[a\x15\xA2V[4\x80\x15a\x04\xF4W__\xFD[Pa\x02\x7F_Q` a\"._9_Q\x90_R\x81V[_`\x01`\x01`\xE0\x1B\x03\x19\x82\x16cye\xDB\x0B`\xE0\x1B\x14\x80a\x059WPc\x01\xFF\xC9\xA7`\xE0\x1B`\x01`\x01`\xE0\x1B\x03\x19\x83\x16\x14[\x92\x91PPV[_\x7F\xD5e\xE3\xFC\x06m\xF3H\xA5\xCB\xC0Z\x8Dc#\xE0\x05R\x83\x80A\xCE\xA2\xD8L\xC5\x98v\xBA7s]a\x05j\x81a\x15\xE4V[`\x01`\x01`\xA0\x1B\x03\x85\x16;\x15a\x05\x96W`\x03\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x87\x16\x17\x90U[`\x01`\x01`\xA0\x1B\x03\x84\x16;\x15a\x05\xC2W`\x04\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x86\x16\x17\x90U[`\x01`\x01`\xA0\x1B\x03\x83\x16;\x15a\x05\xEEW`\x05\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x85\x16\x17\x90U[`\x03T`\x04T`\x05T`@\x80Q`\x01`\x01`\xA0\x1B\x03\x94\x85\x16\x81R\x92\x84\x16` \x84\x01R\x92\x16\x81\x83\x01R\x90Q\x7F\xFF\x8A\x97\xFD\xA7r\x84\x95\xEE:\\U\x1A\xF3I^K\xA2<\xDD\xA2\xAC\x13\x84\x91\xEF\xF2\x94\x90!\"\xEE\x91\x81\x90\x03``\x01\x90\xA1`\x01\x91P[P\x93\x92PPPV[_Q` a\"._9_Q\x90_Ra\x06f\x81a\x15\xE4V[`\x05T`\x01`\x01`\xA0\x1B\x03\x16a\x06\x8FW`@QcC\x7F:\xC3`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[F\x84\x03a\x07EW`\x05T`@Qca\x8Cwm`\xE1\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\xC3\x18\xEE\xDA\x90a\x06\xC8\x90\x8D\x90\x8D\x90`\x04\x01a\x1F\x0CV[`\xA0`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x06\xE3W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x07\x07\x91\x90a\x1F*V[\x90P\x85`\x01`\x01`\xA0\x1B\x03\x16\x81`@\x01Q`\x01`\x01`\xA0\x1B\x03\x16\x14a\x07?W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[Pa\x08\x16V[`\x05T`@Qc\x7F\x99\xD7\xAF`\xE0\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\x7F\x99\xD7\xAF\x90a\x07w\x90\x8D\x90\x8D\x90`\x04\x01a\x1F\x0CV[_`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x07\x91W=__>=_\xFD[PPPP`@Q=_\x82>`\x1F=\x90\x81\x01`\x1F\x19\x16\x82\x01`@Ra\x07\xB8\x91\x90\x81\x01\x90a .V[\x90P\x85`\x01`\x01`\xA0\x1B\x03\x16\x81`\xA0\x01Q`\x01`\x01`\xA0\x1B\x03\x16\x14a\x07\xF0W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x84\x81`\xE0\x01Q\x14a\x08\x14W`@Qc\x97\xC9\x1Bi`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[P[_a\x08#_\x87\x87\x87a\x15\xEEV[\x90P_a\x08P\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qc;~u\xBB`\xE1\x1B\x81R\x90\x91P`\x01`\x01`\xA0\x1B\x03\x82\x16\x90cv\xFC\xEBv\x90a\x08\x85\x90_\x90\x8B\x90\x8B\x90\x8B\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x08\x9CW__\xFD[PZ\xF1\x15\x80\x15a\x08\xAEW=__>=_\xFD[PPPP\x80`\x01`\x01`\xA0\x1B\x03\x16c\xE3\xF5\xC5R4`\x05_\x90T\x90a\x01\0\n\x90\x04`\x01`\x01`\xA0\x1B\x03\x16\x8E\x8E\x8E\x8E\x8B`@Q\x88c\xFF\xFF\xFF\xFF\x16`\xE0\x1B\x81R`\x04\x01a\x08\xFD\x96\x95\x94\x93\x92\x91\x90a!dV[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\t\x14W__\xFD[PZ\xF1\x15\x80\x15a\t&W=__>=_\xFD[PP`@\x80Q\x8A\x81R`\x01`\x01`\xA0\x1B\x03\x8D\x81\x16` \x83\x01R\x80\x8B\x16\x95P\x8C\x81\x16\x94P\x86\x16\x92P\x7F\xD3.\xFC\xEF\xA1\x19\x97\xEB\xEA\x13\xE4\xC3e\xA2\xA9.\xEF\xC7\xB9e\xA7K\xA1Q\xDA\xDB\x02\xA3\xCDP\x067\x91\x01`@Q\x80\x91\x03\x90\xA4PPPPPPPPPPPV[_\x82\x81R`\x01` \x81\x90R`@\x90\x91 \x01Ta\t\xA0\x81a\x15\xE4V[a\t\xAA\x83\x83a\x16QV[PPPPV[_a\t\xBC\x86\x86\x86a\x16\xC7V[\x90P_a\t\xE9\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qcc\x15\xA1G`\xE1\x1B\x81R`\x04\x81\x01\x89\x90R`$\x81\x01\x88\x90R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`D\x83\x01R\x91\x92P\x90\x82\x16\x90c\xC6+B\x8E\x90`d\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\n;W__\xFD[PZ\xF1\x15\x80\x15a\nMW=__>=_\xFD[PP`@Qc\x19\"\xFD\xDD`\xE2\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`\x04\x83\x01R\x86\x81\x16`$\x83\x01R\x84\x16\x92Pcd\x8B\xF7t\x91P`D\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\n\x99W__\xFD[PZ\xF1\x15\x80\x15a\n\xABW=__>=_\xFD[PP`@Q`\x01`\x01`\xA0\x1B\x03\x86\x81\x16\x82R\x80\x88\x16\x93P\x8A\x92P\x84\x16\x90\x7F!\x13\x85\x9F[i\x83\xA7\x07&\x93*\xB3Q\x8E[\xF4T\xF71\xFD\xB1m\x96\xD8\xAC\x87\n[#\"A\x90` \x01`@Q\x80\x91\x03\x90\xA4PPPPPPPV[`\x01`\x01`\xA0\x1B\x03\x81\x163\x14a\x0B'W`@Qc3K\xD9\x19`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x0B1\x82\x82a\x17\x15V[PPPV[_a\x0BC\x85__\x87a\x15\xEEV[\x90P_a\x0Bp\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qc;~u\xBB`\xE1\x1B\x81R\x90\x91P`\x01`\x01`\xA0\x1B\x03\x82\x16\x90cv\xFC\xEBv\x90a\x0B\xA5\x90\x89\x90_\x90\x81\x90\x8B\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x0B\xBCW__\xFD[PZ\xF1\x15\x80\x15a\x0B\xCEW=__>=_\xFD[PP`@Qc\x19\"\xFD\xDD`\xE2\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`\x04\x83\x01R\x86\x81\x16`$\x83\x01R\x84\x16\x92Pcd\x8B\xF7t\x91P`D\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x0C\x1AW__\xFD[PZ\xF1\x15\x80\x15a\x0C,W=__>=_\xFD[PPPP\x82`\x01`\x01`\xA0\x1B\x03\x16\x84`\x01`\x01`\xA0\x1B\x03\x16\x82`\x01`\x01`\xA0\x1B\x03\x16\x7F$\x8A\xD0\xB6\x175F\xF5\xB6\x8C\xA1\xEA\x84\x93\x87\x1F\xB2 V\x1BW}}_\xDF{\xF4\x04\xC1\xC1\xF8y`@Q`@Q\x80\x91\x03\x90\xA4PPPPPPV[_a\x0C\xB8\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x0C\xB3\x85__\x87a\x15\xEEV[a\x17\x80V[\x93\x92PPPV[_a\x0C\xCC_\x87\x87\x87a\x15\xEEV[\x90P_a\x0C\xF9\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qc;~u\xBB`\xE1\x1B\x81R\x90\x91P`\x01`\x01`\xA0\x1B\x03\x82\x16\x90cv\xFC\xEBv\x90a\r.\x90_\x90\x8B\x90\x8B\x90\x8B\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\rEW__\xFD[PZ\xF1\x15\x80\x15a\rWW=__>=_\xFD[PP`@Qc\x19\"\xFD\xDD`\xE2\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`\x04\x83\x01R\x86\x81\x16`$\x83\x01R\x84\x16\x92Pcd\x8B\xF7t\x91P`D\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\r\xA3W__\xFD[PZ\xF1\x15\x80\x15a\r\xB5W=__>=_\xFD[PPPP\x82`\x01`\x01`\xA0\x1B\x03\x16\x84`\x01`\x01`\xA0\x1B\x03\x16\x82`\x01`\x01`\xA0\x1B\x03\x16\x7F$\x8A\xD0\xB6\x175F\xF5\xB6\x8C\xA1\xEA\x84\x93\x87\x1F\xB2 V\x1BW}}_\xDF{\xF4\x04\xC1\xC1\xF8y`@Q`@Q\x80\x91\x03\x90\xA4PPPPPPPV[a\x0E\x14a\x17\xE8V[a\x0E\x1D_a\x18\x14V[V[_Q` a\"._9_Q\x90_Ra\x0E6\x81a\x15\xE4V[`\x05T`\x01`\x01`\xA0\x1B\x03\x16a\x0E_W`@QcC\x7F:\xC3`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05T`@Qc\x7F\x99\xD7\xAF`\xE0\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\x7F\x99\xD7\xAF\x90a\x0E\x91\x90\x8B\x90\x8B\x90`\x04\x01a\x1F\x0CV[_`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x0E\xABW=__>=_\xFD[PPPP`@Q=_\x82>`\x1F=\x90\x81\x01`\x1F\x19\x16\x82\x01`@Ra\x0E\xD2\x91\x90\x81\x01\x90a .V[\x90Pa\x0E\xE1\x86_\x01Q\x85a\x0C\x82V[`\x01`\x01`\xA0\x1B\x03\x16\x81`\xA0\x01Q`\x01`\x01`\xA0\x1B\x03\x16\x14a\x0F\x16W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\xA4\xB1\x81`\xE0\x01Q\x14a\x0F<W`@Qc\x97\xC9\x1Bi`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_a\x0FL\x87_\x01Q__\x88a\x15\xEEV[\x90P_a\x0Fy\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[\x88Q`@Qc;~u\xBB`\xE1\x1B\x81R\x91\x92P`\x01`\x01`\xA0\x1B\x03\x83\x16\x91cv\xFC\xEBv\x91a\x0F\xAE\x91_\x90\x81\x90\x8C\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x0F\xC5W__\xFD[PZ\xF1\x15\x80\x15a\x0F\xD7W=__>=_\xFD[PPPP\x80`\x01`\x01`\xA0\x1B\x03\x16c\xE3\xF5\xC5R4`\x05_\x90T\x90a\x01\0\n\x90\x04`\x01`\x01`\xA0\x1B\x03\x16\x8D\x8D\x8D`@\x01Q\x8D\x8C`@Q\x88c\xFF\xFF\xFF\xFF\x16`\xE0\x1B\x81R`\x04\x01a\x10*\x96\x95\x94\x93\x92\x91\x90a!dV[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x10AW__\xFD[PZ\xF1\x15\x80\x15a\x10SW=__>=_\xFD[PP\x8AQ`@Q`\x01`\x01`\xA0\x1B\x03\x8C\x81\x16\x82R\x80\x8C\x16\x95P\x91\x93P\x90\x85\x16\x91P\x7F\rX\x0C\x10\x96\x0B\xD9\xB3\t\x08\xDAN\xDF\x8E_[\xA6\xB1\xFB\xAF,2\xD2\x8B\xE1u\xA9-K\x98\x96\xF2\x90` \x01`@Q\x80\x91\x03\x90\xA4PPPPPPPPPPV[_\x91\x82R`\x01` \x90\x81R`@\x80\x84 `\x01`\x01`\xA0\x1B\x03\x93\x90\x93\x16\x84R\x91\x90R\x90 T`\xFF\x16\x90V[_Q` a\"._9_Q\x90_Ra\x10\xEE\x81a\x15\xE4V[`\x03T`\x01`\x01`\xA0\x1B\x03\x16\x15\x80a\x11\x0FWP`\x04T`\x01`\x01`\xA0\x1B\x03\x16\x15[\x15a\x11-W`@Qc\x89\xDAqO`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_a\x11=\x84_\x01Q__\x86a\x15\xEEV[\x90P_a\x11j\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[\x85Q`@Qc;~u\xBB`\xE1\x1B\x81R\x91\x92P`\x01`\x01`\xA0\x1B\x03\x83\x16\x91cv\xFC\xEBv\x91a\x11\x9F\x91_\x90\x81\x90\x8A\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x11\xB6W__\xFD[PZ\xF1\x15\x80\x15a\x11\xC8W=__>=_\xFD[PPP`\x01`\x01`\xA0\x1B\x03\x80\x83\x16_\x81\x81R`\x06` R`@\x90\x81\x90 \x80T`\xFF\x19\x16`\x01\x17\x90U`\x04\x80T`\x03T\x92Qc\x0C\xCE*'`\xE1\x1B\x81R\x93\x95Pc\x19\x9CTN\x94a\x12!\x94\x8C\x94\x92\x82\x16\x93\x92\x90\x91\x16\x91\x01a!\xAFV[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x128W__\xFD[PZ\xF1\x15\x80\x15a\x12JW=__>=_\xFD[PPPP\x83`\x01`\x01`\xA0\x1B\x03\x16\x85_\x01Q\x82`\x01`\x01`\xA0\x1B\x03\x16\x7FqS\xFC\x1B\xA1\xD8\xA6xN\n:\xA1\x11>1\x9E\xBFg\x02\xCC\xBD\xCC\x83\xA1&\xA5\xD2;&q\xB0t`@Q`@Q\x80\x91\x03\x90\xA4PPPPPV[_Q` a\"._9_Q\x90_Ra\x12\xB0\x81a\x15\xE4V[`\x05T`\x01`\x01`\xA0\x1B\x03\x16a\x12\xD9W`@QcC\x7F:\xC3`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05T`@Qc\x7F\x99\xD7\xAF`\xE0\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\x7F\x99\xD7\xAF\x90a\x13\x0B\x90\x8D\x90\x8D\x90`\x04\x01a\x1F\x0CV[_`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x13%W=__>=_\xFD[PPPP`@Q=_\x82>`\x1F=\x90\x81\x01`\x1F\x19\x16\x82\x01`@Ra\x13L\x91\x90\x81\x01\x90a .V[\x90P\x84\x81`\xE0\x01Q\x14a\x13rW`@Qc\x97\xC9\x1Bi`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x85a\x13\x83\x8B\x8B\x84a\x01\0\x01Qa\x18cV[\x14a\x13\xA1W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_a\x13\xAD\x87\x87\x87a\x16\xC7V[\x90P_a\x13\xDA\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qcc\x15\xA1G`\xE1\x1B\x81R`\x04\x81\x01\x8A\x90R`$\x81\x01\x89\x90R`\x01`\x01`\xA0\x1B\x03\x88\x81\x16`D\x83\x01R\x91\x92P\x90\x82\x16\x90c\xC6+B\x8E\x90`d\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x14,W__\xFD[PZ\xF1\x15\x80\x15a\x14>W=__>=_\xFD[PPPP\x80`\x01`\x01`\xA0\x1B\x03\x16c\xE3\xF5\xC5R4`\x05_\x90T\x90a\x01\0\n\x90\x04`\x01`\x01`\xA0\x1B\x03\x16\x8F\x8F\x8F\x8F\x8C`@Q\x88c\xFF\xFF\xFF\xFF\x16`\xE0\x1B\x81R`\x04\x01a\x14\x8D\x96\x95\x94\x93\x92\x91\x90a!dV[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x14\xA4W__\xFD[PZ\xF1\x15\x80\x15a\x14\xB6W=__>=_\xFD[PP`@\x80Q\x8B\x81R`\x01`\x01`\xA0\x1B\x03\x8E\x81\x16` \x83\x01R\x80\x8C\x16\x95P\x8D\x94P\x86\x16\x92P\x7F!Y\xA9\xBE\xAC:k\xDFH\x8B\xE5\x8A@\xCBM\xF3\xC7d\x90\x82\x93:)\x0F\x1D\x86\xA4>\xBB\xA2\xD2\x13\x91\x01`@Q\x80\x91\x03\x90\xA4PPPPPPPPPPPPV[_\x82\x81R`\x01` \x81\x90R`@\x90\x91 \x01Ta\x15/\x81a\x15\xE4V[a\t\xAA\x83\x83a\x17\x15V[_a\x15i\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x0C\xB3\x86\x86\x86a\x16\xC7V[\x94\x93PPPPV[_a\x15i\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x0C\xB3_\x87\x87\x87a\x15\xEEV[a\x15\xAAa\x17\xE8V[`\x01`\x01`\xA0\x1B\x03\x81\x16a\x15\xD8W`@Qc\x1EO\xBD\xF7`\xE0\x1B\x81R_`\x04\x82\x01R`$\x01[`@Q\x80\x91\x03\x90\xFD[a\x15\xE1\x81a\x18\x14V[PV[a\x15\xE1\x813a\x18\xC9V[`\x02T`@\x80Q` \x81\x01\x92\x90\x92R\x81\x01\x85\x90R`\x01`\x01`\xA0\x1B\x03\x80\x85\x16``\x83\x01R`\x80\x82\x01\x84\x90R\x82\x16`\xA0\x82\x01R_\x90`\xC0\x01`@Q` \x81\x83\x03\x03\x81R\x90`@R\x80Q\x90` \x01 \x90P\x94\x93PPPPV[_a\x0C\xB8\x83\x83_a\x19\x06V[_a\x16\\\x83\x83a\x10\xADV[a\x16\xC0W_\x83\x81R`\x01` \x81\x81R`@\x80\x84 `\x01`\x01`\xA0\x1B\x03\x87\x16\x80\x86R\x92R\x80\x84 \x80T`\xFF\x19\x16\x90\x93\x17\x90\x92U\x90Q3\x92\x86\x91\x7F/\x87\x88\x11~~\xFF\x1D\x82\xE9&\xECyI\x01\xD1|x\x02JP'\t@0E@\xA73eo\r\x91\x90\xA4P`\x01a\x059V[P_a\x059V[`\x02T`@\x80Q` \x81\x01\x92\x90\x92R\x81\x01\x84\x90R``\x81\x01\x83\x90R`\x01`\x01`\xA0\x1B\x03\x82\x16`\x80\x82\x01R_\x90`\xA0\x01`@Q` \x81\x83\x03\x03\x81R\x90`@R\x80Q\x90` \x01 \x90P\x93\x92PPPV[_a\x17 \x83\x83a\x10\xADV[\x15a\x16\xC0W_\x83\x81R`\x01` \x90\x81R`@\x80\x83 `\x01`\x01`\xA0\x1B\x03\x86\x16\x80\x85R\x92R\x80\x83 \x80T`\xFF\x19\x16\x90UQ3\x92\x86\x91\x7F\xF69\x1F\\2\xD9\xC6\x9D*G\xEAg\x0BD)t\xB595\xD1\xED\xC7\xFDd\xEB!\xE0G\xA89\x17\x1B\x91\x90\xA4P`\x01a\x059V[`@Q0`8\x82\x01RoZ\xF4=\x82\x80>\x90=\x91`+W\xFD[\xF3\xFF`$\x82\x01R`\x14\x81\x01\x83\x90Rs=`-\x80`\n=9\x81\xF36==7===6=s\x81R`X\x81\x01\x82\x90R`7`\x0C\x82\x01 `x\x82\x01R`U`C\x90\x91\x01 _\x90`\x01`\x01`\xA0\x1B\x03\x16a\x0C\xB8V[_T`\x01`\x01`\xA0\x1B\x03\x163\x14a\x0E\x1DW`@Qc\x11\x8C\xDA\xA7`\xE0\x1B\x81R3`\x04\x82\x01R`$\x01a\x15\xCFV[_\x80T`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`\x01`\x01`\xA0\x1B\x03\x19\x83\x16\x81\x17\x84U`@Q\x91\x90\x92\x16\x92\x83\x91\x7F\x8B\xE0\x07\x9CS\x16Y\x14\x13D\xCD\x1F\xD0\xA4\xF2\x84\x19I\x7F\x97\"\xA3\xDA\xAF\xE3\xB4\x18okdW\xE0\x91\x90\xA3PPV[__\x84\x84\x80\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x93\x92\x91\x90\x81\x81R` \x01\x83\x83\x80\x82\x847_\x92\x01\x91\x90\x91RP\x92\x93PP\x84\x15\x91Pa\x18\xB7\x90PW`d\x81\x01Q\x81\x01`$\x01Q\x91Pa\x06GV[`D\x81\x01Q\x01`$\x01Q\x94\x93PPPPV[a\x18\xD3\x82\x82a\x10\xADV[a\x19\x02W`@Qc\xE2Q}?`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x82\x16`\x04\x82\x01R`$\x81\x01\x83\x90R`D\x01a\x15\xCFV[PPV[_\x81G\x10\x15a\x191W`@Qc\xCFG\x91\x81`\xE0\x1B\x81RG`\x04\x82\x01R`$\x81\x01\x83\x90R`D\x01a\x15\xCFV[v=`-\x80`\n=9\x81\xF36==7===6=s\0\0\0\x84``\x1B`\xE8\x1C\x17_RnZ\xF4=\x82\x80>\x90=\x91`+W\xFD[\xF3\x84`x\x1B\x17` R\x82`7`\t\x84\xF5\x90P`\x01`\x01`\xA0\x1B\x03\x81\x16a\x0C\xB8W`@Qc\xB0n\xBF=`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_` \x82\x84\x03\x12\x15a\x19\xABW__\xFD[\x815`\x01`\x01`\xE0\x1B\x03\x19\x81\x16\x81\x14a\x0C\xB8W__\xFD[`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x15\xE1W__\xFD[___``\x84\x86\x03\x12\x15a\x19\xE8W__\xFD[\x835a\x19\xF3\x81a\x19\xC2V[\x92P` \x84\x015a\x1A\x03\x81a\x19\xC2V[\x91P`@\x84\x015a\x1A\x13\x81a\x19\xC2V[\x80\x91PP\x92P\x92P\x92V[_` \x82\x84\x03\x12\x15a\x1A.W__\xFD[P5\x91\x90PV[__\x83`\x1F\x84\x01\x12a\x1AEW__\xFD[P\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1A\\W__\xFD[` \x83\x01\x91P\x83` \x82\x85\x01\x01\x11\x15a\x1AsW__\xFD[\x92P\x92\x90PV[________`\xE0\x89\x8B\x03\x12\x15a\x1A\x91W__\xFD[\x885g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1A\xA7W__\xFD[a\x1A\xB3\x8B\x82\x8C\x01a\x1A5V[\x90\x99P\x97PP` \x89\x015\x95P`@\x89\x015a\x1A\xCE\x81a\x19\xC2V[\x94P``\x89\x015a\x1A\xDE\x81a\x19\xC2V[\x93P`\x80\x89\x015\x92P`\xA0\x89\x015a\x1A\xF5\x81a\x19\xC2V[\x97\x9A\x96\x99P\x94\x97\x93\x96\x92\x95\x91\x94P\x91\x92`\xC0\x015\x91PV[__`@\x83\x85\x03\x12\x15a\x1B\x1EW__\xFD[\x825\x91P` \x83\x015a\x1B0\x81a\x19\xC2V[\x80\x91PP\x92P\x92\x90PV[_____`\xA0\x86\x88\x03\x12\x15a\x1BOW__\xFD[\x855\x94P` \x86\x015\x93P`@\x86\x015a\x1Bh\x81a\x19\xC2V[\x92P``\x86\x015a\x1Bx\x81a\x19\xC2V[\x91P`\x80\x86\x015a\x1B\x88\x81a\x19\xC2V[\x80\x91PP\x92\x95P\x92\x95\x90\x93PV[____`\x80\x85\x87\x03\x12\x15a\x1B\xA9W__\xFD[\x845\x93P` \x85\x015a\x1B\xBB\x81a\x19\xC2V[\x92P`@\x85\x015a\x1B\xCB\x81a\x19\xC2V[\x91P``\x85\x015a\x1B\xDB\x81a\x19\xC2V[\x93\x96\x92\x95P\x90\x93PPV[_____`\xA0\x86\x88\x03\x12\x15a\x1B\xFAW__\xFD[\x855a\x1C\x05\x81a\x19\xC2V[\x94P` \x86\x015\x93P`@\x86\x015a\x1Bh\x81a\x19\xC2V[cNH{q`\xE0\x1B_R`A`\x04R`$_\xFD[`@Q`\xA0\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1CSWa\x1CSa\x1C\x1CV[`@R\x90V[`@\x80Q\x90\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1CSWa\x1CSa\x1C\x1CV[`@Qa\x01@\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1CSWa\x1CSa\x1C\x1CV[\x805a\xFF\xFF\x81\x16\x81\x14a\x1C\xB1W__\xFD[\x91\x90PV[_`\xC0\x82\x84\x03\x12\x15a\x1C\xC6W__\xFD[a\x1C\xCEa\x1C0V[\x825\x81R` \x80\x84\x015\x90\x82\x01R`@\x80\x84\x015\x90\x82\x01R\x90P`\x7F\x82\x01\x83\x13a\x1C\xF6W__\xFD[a\x1C\xFEa\x1CYV[\x80`\xA0\x84\x01\x85\x81\x11\x15a\x1D\x0FW__\xFD[``\x85\x01[\x81\x81\x10\x15a\x1D,W\x805\x84R` \x93\x84\x01\x93\x01a\x1D\x14V[P\x81``\x85\x01Ra\x1D<\x81a\x1C\xA0V[`\x80\x85\x01RPPP\x92\x91PPV[______a\x01@\x87\x89\x03\x12\x15a\x1D`W__\xFD[\x865g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1DvW__\xFD[a\x1D\x82\x89\x82\x8A\x01a\x1A5V[\x90\x97P\x95Pa\x1D\x96\x90P\x88` \x89\x01a\x1C\xB6V[\x93P`\xE0\x87\x015a\x1D\xA6\x81a\x19\xC2V[\x92Pa\x01\0\x87\x015a\x1D\xB7\x81a\x19\xC2V[\x95\x98\x94\x97P\x92\x95\x91\x94\x93a\x01 \x90\x92\x015\x92PPV[__`\xE0\x83\x85\x03\x12\x15a\x1D\xDEW__\xFD[a\x1D\xE8\x84\x84a\x1C\xB6V[\x91P`\xC0\x83\x015a\x1B0\x81a\x19\xC2V[________`\xE0\x89\x8B\x03\x12\x15a\x1E\x0FW__\xFD[\x885g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1E%W__\xFD[a\x1E1\x8B\x82\x8C\x01a\x1A5V[\x90\x99P\x97PP` \x89\x015\x95P`@\x89\x015a\x1EL\x81a\x19\xC2V[\x94P``\x89\x015\x93P`\x80\x89\x015\x92P`\xA0\x89\x015a\x1A\xF5\x81a\x19\xC2V[___``\x84\x86\x03\x12\x15a\x1E|W__\xFD[\x835\x92P` \x84\x015\x91P`@\x84\x015a\x1A\x13\x81a\x19\xC2V[___``\x84\x86\x03\x12\x15a\x1E\xA7W__\xFD[\x835a\x1E\xB2\x81a\x19\xC2V[\x92P` \x84\x015\x91P`@\x84\x015a\x1A\x13\x81a\x19\xC2V[_` \x82\x84\x03\x12\x15a\x1E\xD9W__\xFD[\x815a\x0C\xB8\x81a\x19\xC2V[\x81\x83R\x81\x81` \x85\x017P_\x82\x82\x01` \x90\x81\x01\x91\x90\x91R`\x1F\x90\x91\x01`\x1F\x19\x16\x90\x91\x01\x01\x90V[` \x81R_a\x15i` \x83\x01\x84\x86a\x1E\xE4V[\x80Qa\x1C\xB1\x81a\x19\xC2V[_`\xA0\x82\x84\x03\x12\x80\x15a\x1F;W__\xFD[Pa\x1FDa\x1C0V[\x82Qa\x1FO\x81a\x19\xC2V[\x81R` \x83\x81\x01Q\x90\x82\x01R`@\x83\x01Qa\x1Fi\x81a\x19\xC2V[`@\x82\x01R``\x83\x01Qa\x1F|\x81a\x19\xC2V[``\x82\x01R`\x80\x92\x83\x01Q\x92\x81\x01\x92\x90\x92RP\x91\x90PV[_\x82`\x1F\x83\x01\x12a\x1F\xA3W__\xFD[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1F\xBDWa\x1F\xBDa\x1C\x1CV[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1F\xECWa\x1F\xECa\x1C\x1CV[`@R\x81\x81R\x83\x82\x01` \x01\x85\x10\x15a \x03W__\xFD[\x81` \x85\x01` \x83\x01^_\x91\x81\x01` \x01\x91\x90\x91R\x93\x92PPPV[\x80Q\x80\x15\x15\x81\x14a\x1C\xB1W__\xFD[_` \x82\x84\x03\x12\x15a >W__\xFD[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a TW__\xFD[\x82\x01a\x01@\x81\x85\x03\x12\x15a fW__\xFD[a na\x1C|V[\x81Q\x81R` \x82\x01Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a \x8BW__\xFD[a \x97\x86\x82\x85\x01a\x1F\x94V[` \x83\x01RP`@\x82\x01Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a \xB6W__\xFD[a \xC2\x86\x82\x85\x01a\x1F\x94V[`@\x83\x01RPa \xD4``\x83\x01a\x1F\x1FV[``\x82\x01Ra \xE5`\x80\x83\x01a\x1F\x1FV[`\x80\x82\x01Ra \xF6`\xA0\x83\x01a\x1F\x1FV[`\xA0\x82\x01R`\xC0\x82\x81\x01Q\x90\x82\x01R`\xE0\x80\x83\x01Q\x90\x82\x01Ra!\x1Ca\x01\0\x83\x01a \x1FV[a\x01\0\x82\x01Ra!/a\x01 \x83\x01a \x1FV[a\x01 \x82\x01R\x94\x93PPPPV[\x93\x84R`\x01`\x01`\xA0\x1B\x03\x92\x83\x16` \x85\x01R`@\x84\x01\x91\x90\x91R\x16``\x82\x01R`\x80\x01\x90V[`\x01`\x01`\xA0\x1B\x03\x87\x16\x81R`\xA0` \x82\x01\x81\x90R_\x90a!\x88\x90\x83\x01\x87\x89a\x1E\xE4V[`@\x83\x01\x95\x90\x95RP`\x01`\x01`\xA0\x1B\x03\x92\x90\x92\x16``\x83\x01R`\x80\x90\x91\x01R\x93\x92PPPV[_a\x01\0\x82\x01\x90P\x84Q\x82R` \x85\x01Q` \x83\x01R`@\x85\x01Q`@\x83\x01R``\x85\x01Q``\x83\x01_[`\x02\x81\x10\x15a!\xF9W\x82Q\x82R` \x92\x83\x01\x92\x90\x91\x01\x90`\x01\x01a!\xDAV[PPP`\x80\x85\x01Qa\xFF\xFF\x16`\xA0\x83\x01R`\x01`\x01`\xA0\x1B\x03\x84\x16`\xC0\x83\x01R`\x01`\x01`\xA0\x1B\x03\x83\x16`\xE0\x83\x01Ra\x15iV\xFE\x97fpp\xC5N\xF1\x82\xB0\xF5\x85\x8B\x03K\xEA\xC1\xB6\xF3\x08\x9A\xA2\xD3\x18\x8B\xB1\xE8\x92\x9FO\xA9\xB9)\xA2dipfsX\"\x12 \x14\x92\xFE\xF9\x15\xBA\xF9\xE1\x9F\xE3\xE5>6`8\x90\x94<\xF5\x88\xD0\xD7y\xF0\xC7\x1D\xB4\x183\x07WjdsolcC\0\x08\x1C\x003`\x80`@R4\x80\x15`\x0EW__\xFD[P`\x05\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90Ua\x0F\x05\x80a\0/_9_\xF3\xFE`\x80`@R`\x046\x10a\0nW_5`\xE0\x1C\x80cd\x8B\xF7t\x11a\0LW\x80cd\x8B\xF7t\x14a\0\xEDW\x80cv\xFC\xEBv\x14a\x01\x0CW\x80c\xDD\xCE\xAF\xA9\x14a\x01+W\x80c\xE3\xF5\xC5R\x14a\x01JW__\xFD[\x80c\x19\x9CTN\x14a\0rW\x80c?\xB0p'\x14a\0\x93W\x80c]\xC2N\xE3\x14a\0\xCEW[__\xFD[4\x80\x15a\0}W__\xFD[Pa\0\x91a\0\x8C6`\x04a\x0C!V[a\x01]V[\0[4\x80\x15a\0\x9EW__\xFD[P`\x04Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01`@Q\x80\x91\x03\x90\xF3[4\x80\x15a\0\xD9W__\xFD[P`\x03Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[4\x80\x15a\0\xF8W__\xFD[Pa\0\x91a\x01\x076`\x04a\x0C\xE6V[a\x03\xA2V[4\x80\x15a\x01\x17W__\xFD[Pa\0\x91a\x01&6`\x04a\r\x1DV[a\x05\xB9V[4\x80\x15a\x016W__\xFD[P`\x05Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[a\0\x91a\x01X6`\x04a\rdV[a\x06\x8EV[`\x05T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x01\xB6W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01Rv\x14\xDA[\x99\xDB\x19U\\\xD9N\x88\x10[\x1C\x99XY\x1EH\x1D\\\xD9Y`J\x1B`D\x82\x01R`d\x01[`@Q\x80\x91\x03\x90\xFD[_T\x83Q\x14a\x01\xD7W`@Qb\xCB}\xFF`\xE8\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x03\x80T`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`\x01`\x01`\xA0\x1B\x03\x19\x92\x83\x16\x17\x90\x92U`\x04\x80T\x92\x84\x16\x92\x90\x91\x16\x82\x17\x81U` \x85\x01Q`@Qc\x0C\xF9\x9B\xE7`\xE3\x1B\x81R\x91\x82\x01R_\x91\x90cg\xCC\xDF8\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x02BW=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x02f\x91\x90a\x0E\x08V[\x90P`\x01`\x01`\xA0\x1B\x03\x81\x16\x15\x80\x15\x90a\x02\x9DWP`\x01`\x01`\xA0\x1B\x03\x81\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x03$W`\x03T`@\x85\x01Qa\x02\xC2\x91`\x01`\x01`\xA0\x1B\x03\x84\x81\x16\x92\x91\x16\x90a\t\xDFV[`\x03T`@Qc\xBAH\xD1\x17`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90c\xBAH\xD1\x17\x90a\x02\xF2\x90\x87\x90`\x04\x01a\x0E*V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x03\tW__\xFD[PZ\xF1\x15\x80\x15a\x03\x1BW=__>=_\xFD[PPPPa\x03\x89V[`\x03T`@\x80\x86\x01Q\x90Qc\xBAH\xD1\x17`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x92\x16\x91c\xBAH\xD1\x17\x91\x90a\x03Z\x90\x88\x90`\x04\x01a\x0E*V[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x03qW__\xFD[PZ\xF1\x15\x80\x15a\x03\x83W=__>=_\xFD[PPPPP[PP`\x05\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPV[`\x05T`\x01`\x01`\xA0\x1B\x03\x162\x14a\x03\xF4W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x15`$\x82\x01RtPortal: Only recovery`X\x1B`D\x82\x01R`d\x01a\x01\xADV[`\x01`\x01`\xA0\x1B\x03\x81\x16a\x04\x1BW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x80a\x04MWP`\x01`\x01`\xA0\x1B\x03\x82\x16\x15[\x15a\x05\x19WG_\x81\x90\x03a\x04tW`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x82`\x01`\x01`\xA0\x1B\x03\x16\x82`@Q_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x04\xBDW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x04\xC2V[``\x91P[PP\x90P\x80a\x05\x13W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1B`$\x82\x01R\x7FPortal: ETH transfer failed\0\0\0\0\0`D\x82\x01R`d\x01a\x01\xADV[PPPPV[`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x05_W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x05\x83\x91\x90a\x0E\x8AV[\x90P\x80_\x03a\x05\xA5W`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x05\x13`\x01`\x01`\xA0\x1B\x03\x83\x16\x84\x83a\n\x9CV[`\x05T`\x01`\xA8\x1B\x90\x04`\xFF\x16\x15a\x05\xE3W`@Qb\xDC\x14\x9F`\xE4\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90U`\x01`\x01`\xA0\x1B\x03\x81\x16a\x06\x1DW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x83\x15`\x01`\x01`\xA0\x1B\x03\x84\x16\x15\x14\x80a\x067WP\x83\x15\x82\x15\x14[\x15a\x06UW`@Qc\x1DM\xEB\x8B`\xE3\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x93\x90\x93U`\x01\x80T`\x01`\x01`\xA0\x1B\x03\x93\x84\x16`\x01`\x01`\xA0\x1B\x03\x19\x91\x82\x16\x17\x90\x91U`\x02\x91\x90\x91U`\x05\x80T\x92\x90\x93\x16\x91\x16\x17\x90UV[`\x05T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x06\xE2W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01Rv\x14\xDA[\x99\xDB\x19U\\\xD9N\x88\x10[\x1C\x99XY\x1EH\x1D\\\xD9Y`J\x1B`D\x82\x01R`d\x01a\x01\xADV[`\x01`\x01`\xA0\x1B\x03\x82\x16\x15\x80\x15\x90a\x07\x17WP`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x08\x83W`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x07bW=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x07\x86\x91\x90a\x0E\x8AV[\x90P\x84\x81\x10\x15a\x07\xA9W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x07\xBD`\x01`\x01`\xA0\x1B\x03\x83\x16\x89\x87a\t\xDFV[__\x89`\x01`\x01`\xA0\x1B\x03\x164\x8A\x8A`@Qa\x07\xDA\x92\x91\x90a\x0E\xA1V[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x08\x14W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x08\x19V[``\x91P[P\x90\x92P\x90Pa\x083`\x01`\x01`\xA0\x1B\x03\x85\x16\x8B_a\t\xDFV[\x84\x15a\x08MWa\x08M`\x01`\x01`\xA0\x1B\x03\x85\x162\x87a\n\x9CV[\x81a\x08zW\x80Q\x15a\x08aW\x80Q\x81` \x01\xFD[`@Qc\x1B\xB7\xDA\xAD`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[PPPPa\t\xC4V[G\x83\x81\x10\x15a\x08\xA5W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x80`\x01`\x01`\xA0\x1B\x03\x89\x16a\x08\xBB4\x88a\x0E\xB0V[\x89\x89`@Qa\x08\xCB\x92\x91\x90a\x0E\xA1V[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\t\x05W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\t\nV[``\x91P[P\x91P\x91P\x81a\t#W\x80Q\x15a\x08aW\x80Q\x81` \x01\xFD[\x83\x15a\t\xC0W`@Q_\x902\x90\x86\x90\x83\x81\x81\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\thW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\tmV[``\x91P[PP\x90P\x80a\t\xBEW`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1C`$\x82\x01R\x7FPortal: reimbursement failed\0\0\0\0`D\x82\x01R`d\x01a\x01\xADV[P[PPP[PP`\x05\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPPPV[`@\x80Q`\x01`\x01`\xA0\x1B\x03\x84\x16`$\x82\x01R`D\x80\x82\x01\x84\x90R\x82Q\x80\x83\x03\x90\x91\x01\x81R`d\x90\x91\x01\x90\x91R` \x81\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x16c\t^\xA7\xB3`\xE0\x1B\x17\x90Ra\n0\x84\x82a\n\xD2V[a\x05\x13W`@Q`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`$\x83\x01R_`D\x83\x01Ra\n\x92\x91\x86\x91\x82\x16\x90c\t^\xA7\xB3\x90`d\x01[`@Q` \x81\x83\x03\x03\x81R\x90`@R\x91P`\xE0\x1B` \x82\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x83\x81\x83\x16\x17\x83RPPPPa\x0B\x1DV[a\x05\x13\x84\x82a\x0B\x1DV[`@Q`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`$\x83\x01R`D\x82\x01\x83\x90Ra\n\xCD\x91\x85\x91\x82\x16\x90c\xA9\x05\x9C\xBB\x90`d\x01a\n`V[PPPV[____` _\x86Q` \x88\x01_\x8AZ\xF1\x92P=\x91P_Q\x90P\x82\x80\x15a\x0B\x11WP\x81\x15a\x0B\x03W\x80`\x01\x14a\x0B\x11V[_\x86`\x01`\x01`\xA0\x1B\x03\x16;\x11[\x93PPPP[\x92\x91PPV[__` _\x84Q` \x86\x01_\x88Z\xF1\x80a\x0B<W`@Q=_\x82>=\x81\xFD[PP_Q=\x91P\x81\x15a\x0BSW\x80`\x01\x14\x15a\x0B`V[`\x01`\x01`\xA0\x1B\x03\x84\x16;\x15[\x15a\x05\x13W`@QcRt\xAF\xE7`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x85\x16`\x04\x82\x01R`$\x01a\x01\xADV[cNH{q`\xE0\x1B_R`A`\x04R`$_\xFD[`@Q`\xA0\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x0B\xC0Wa\x0B\xC0a\x0B\x89V[`@R\x90V[`@\x80Q\x90\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x0B\xC0Wa\x0B\xC0a\x0B\x89V[\x805a\xFF\xFF\x81\x16\x81\x14a\x0B\xFAW__\xFD[\x91\x90PV[`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x0C\x13W__\xFD[PV[\x805a\x0B\xFA\x81a\x0B\xFFV[___\x83\x85\x03a\x01\0\x81\x12\x15a\x0C5W__\xFD[`\xC0\x81\x12\x15a\x0CBW__\xFD[Pa\x0CKa\x0B\x9DV[\x845\x81R` \x80\x86\x015\x90\x82\x01R`@\x80\x86\x015\x90\x82\x01R`\x7F\x85\x01\x86\x13a\x0CqW__\xFD[a\x0Cya\x0B\xC6V[\x80`\xA0\x87\x01\x88\x81\x11\x15a\x0C\x8AW__\xFD[``\x88\x01[\x81\x81\x10\x15a\x0C\xA7W\x805\x84R` \x93\x84\x01\x93\x01a\x0C\x8FV[P\x81``\x85\x01Ra\x0C\xB7\x81a\x0B\xE9V[`\x80\x85\x01RPPP\x80\x93PPa\x0C\xCF`\xC0\x85\x01a\x0C\x16V[\x91Pa\x0C\xDD`\xE0\x85\x01a\x0C\x16V[\x90P\x92P\x92P\x92V[__`@\x83\x85\x03\x12\x15a\x0C\xF7W__\xFD[\x825a\r\x02\x81a\x0B\xFFV[\x91P` \x83\x015a\r\x12\x81a\x0B\xFFV[\x80\x91PP\x92P\x92\x90PV[____`\x80\x85\x87\x03\x12\x15a\r0W__\xFD[\x845\x93P` \x85\x015a\rB\x81a\x0B\xFFV[\x92P`@\x85\x015\x91P``\x85\x015a\rY\x81a\x0B\xFFV[\x93\x96\x92\x95P\x90\x93PPV[______`\xA0\x87\x89\x03\x12\x15a\ryW__\xFD[\x865a\r\x84\x81a\x0B\xFFV[\x95P` \x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\r\x9FW__\xFD[\x87\x01`\x1F\x81\x01\x89\x13a\r\xAFW__\xFD[\x805g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\r\xC5W__\xFD[\x89` \x82\x84\x01\x01\x11\x15a\r\xD6W__\xFD[` \x91\x90\x91\x01\x95P\x93P`@\x87\x015\x92Pa\r\xF3``\x88\x01a\x0C\x16V[\x95\x98\x94\x97P\x92\x95\x91\x94\x93`\x80\x90\x92\x015\x92PPV[_` \x82\x84\x03\x12\x15a\x0E\x18W__\xFD[\x81Qa\x0E#\x81a\x0B\xFFV[\x93\x92PPPV[_`\xC0\x82\x01\x90P\x82Q\x82R` \x83\x01Q` \x83\x01R`@\x83\x01Q`@\x83\x01R``\x83\x01Q``\x83\x01_[`\x02\x81\x10\x15a\x0EsW\x82Q\x82R` \x92\x83\x01\x92\x90\x91\x01\x90`\x01\x01a\x0ETV[PPPa\xFF\xFF`\x80\x84\x01Q\x16`\xA0\x83\x01R\x92\x91PPV[_` \x82\x84\x03\x12\x15a\x0E\x9AW__\xFD[PQ\x91\x90PV[\x81\x83\x827_\x91\x01\x90\x81R\x91\x90PV[\x80\x82\x01\x80\x82\x11\x15a\x0B\x17WcNH{q`\xE0\x1B_R`\x11`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 ,5\x9CVv\xBF\xF8\xBB\x9B\xE36\xFB.\x05'\xB43}\x8C\x88NL\x9E\x03g\x9ATo{\xDA\x8B\xEAdsolcC\0\x08\x1C\x003`\x80`@R4\x80\x15`\x0EW__\xFD[P`\x02\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90Ua\t\x89\x80a\0/_9_\xF3\xFE`\x80`@R`\x046\x10a\0>W_5`\xE0\x1C\x80cd\x8B\xF7t\x14a\0BW\x80c\xC6+B\x8E\x14a\0cW\x80c\xDD\xCE\xAF\xA9\x14a\0\x82W\x80c\xE3\xF5\xC5R\x14a\0\xBDW[__\xFD[4\x80\x15a\0MW__\xFD[Pa\0aa\0\\6`\x04a\x08(V[a\0\xD0V[\0[4\x80\x15a\0nW__\xFD[Pa\0aa\0}6`\x04a\x08YV[a\x03\x02V[4\x80\x15a\0\x8DW__\xFD[P`\x02Ta\0\xA1\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01`@Q\x80\x91\x03\x90\xF3[a\0aa\0\xCB6`\x04a\x08\x8BV[a\x03\xB8V[`\x02T`\x01`\x01`\xA0\x1B\x03\x162\x14a\x01/W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1F`$\x82\x01R\x7FPortalSolanaExit: Only recovery\0`D\x82\x01R`d\x01[`@Q\x80\x91\x03\x90\xFD[`\x01`\x01`\xA0\x1B\x03\x81\x16a\x01VW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x80a\x01\x88WP`\x01`\x01`\xA0\x1B\x03\x82\x16\x15[\x15a\x02bWG_\x81\x90\x03a\x01\xAFW`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x82`\x01`\x01`\xA0\x1B\x03\x16\x82`@Q_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x01\xF8W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x01\xFDV[``\x91P[PP\x90P\x80a\x02\\W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`%`$\x82\x01R\x7FPortalSolanaExit: ETH transfer f`D\x82\x01Rd\x18Z[\x19Y`\xDA\x1B`d\x82\x01R`\x84\x01a\x01&V[PPPPV[`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x02\xA8W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x02\xCC\x91\x90a\t-V[\x90P\x80_\x03a\x02\xEEW`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x02\\`\x01`\x01`\xA0\x1B\x03\x83\x16\x84\x83a\x06iV[`\x02T`\x01`\xA8\x1B\x90\x04`\xFF\x16\x15a\x03,W`@Qb\xDC\x14\x9F`\xE4\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x02\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90U`\x01`\x01`\xA0\x1B\x03\x81\x16a\x03fW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x82\x15\x80a\x03qWP\x81\x15[\x15a\x03\x8FW`@Qc\x7F\xA4f\xF9`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x92\x90\x92U`\x01U`\x02\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x90\x92\x16\x91\x90\x91\x17\x90UV[`\x02T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x04\x12W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01R\x7FSingleUse: Already used\0\0\0\0\0\0\0\0\0`D\x82\x01R`d\x01a\x01&V[`\x01`\x01`\xA0\x1B\x03\x82\x16\x15\x80\x15\x90a\x04GWP`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x05\xB3W`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x04\x92W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x04\xB6\x91\x90a\t-V[\x90P\x84\x81\x10\x15a\x04\xD9W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x04\xED`\x01`\x01`\xA0\x1B\x03\x83\x16\x89\x87a\x06\xCDV[__\x89`\x01`\x01`\xA0\x1B\x03\x164\x8A\x8A`@Qa\x05\n\x92\x91\x90a\tDV[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x05DW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x05IV[``\x91P[P\x90\x92P\x90Pa\x05c`\x01`\x01`\xA0\x1B\x03\x85\x16\x8B_a\x06\xCDV[\x84\x15a\x05}Wa\x05}`\x01`\x01`\xA0\x1B\x03\x85\x163\x87a\x06iV[\x81a\x05\xAAW\x80Q\x15a\x05\x91W\x80Q\x81` \x01\xFD[`@Qc\x1B\xB7\xDA\xAD`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[PPPPa\x06NV[G\x83\x81\x10\x15a\x05\xD5W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[__\x88`\x01`\x01`\xA0\x1B\x03\x16\x86\x89\x89`@Qa\x05\xF2\x92\x91\x90a\tDV[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x06,W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x061V[``\x91P[P\x91P\x91P\x81a\x06JW\x80Q\x15a\x05\x91W\x80Q\x81` \x01\xFD[PPP[PP`\x02\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPPPV[`@Q`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`$\x83\x01R`D\x82\x01\x83\x90Ra\x06\xC8\x91\x85\x91\x82\x16\x90c\xA9\x05\x9C\xBB\x90`d\x01[`@Q` \x81\x83\x03\x03\x81R\x90`@R\x91P`\xE0\x1B` \x82\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x83\x81\x83\x16\x17\x83RPPPPa\x07XV[PPPV[`@\x80Q`\x01`\x01`\xA0\x1B\x03\x84\x16`$\x82\x01R`D\x80\x82\x01\x84\x90R\x82Q\x80\x83\x03\x90\x91\x01\x81R`d\x90\x91\x01\x90\x91R` \x81\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x16c\t^\xA7\xB3`\xE0\x1B\x17\x90Ra\x07\x1E\x84\x82a\x07\xC4V[a\x02\\W`@Q`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`$\x83\x01R_`D\x83\x01Ra\x07R\x91\x86\x91\x82\x16\x90c\t^\xA7\xB3\x90`d\x01a\x06\x96V[a\x02\\\x84\x82[__` _\x84Q` \x86\x01_\x88Z\xF1\x80a\x07wW`@Q=_\x82>=\x81\xFD[PP_Q=\x91P\x81\x15a\x07\x8EW\x80`\x01\x14\x15a\x07\x9BV[`\x01`\x01`\xA0\x1B\x03\x84\x16;\x15[\x15a\x02\\W`@QcRt\xAF\xE7`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x85\x16`\x04\x82\x01R`$\x01a\x01&V[____` _\x86Q` \x88\x01_\x8AZ\xF1\x92P=\x91P_Q\x90P\x82\x80\x15a\x08\x03WP\x81\x15a\x07\xF5W\x80`\x01\x14a\x08\x03V[_\x86`\x01`\x01`\xA0\x1B\x03\x16;\x11[\x96\x95PPPPPPV[\x805`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x08#W__\xFD[\x91\x90PV[__`@\x83\x85\x03\x12\x15a\x089W__\xFD[a\x08B\x83a\x08\rV[\x91Pa\x08P` \x84\x01a\x08\rV[\x90P\x92P\x92\x90PV[___``\x84\x86\x03\x12\x15a\x08kW__\xFD[\x835\x92P` \x84\x015\x91Pa\x08\x82`@\x85\x01a\x08\rV[\x90P\x92P\x92P\x92V[______`\xA0\x87\x89\x03\x12\x15a\x08\xA0W__\xFD[a\x08\xA9\x87a\x08\rV[\x95P` \x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x08\xC4W__\xFD[\x87\x01`\x1F\x81\x01\x89\x13a\x08\xD4W__\xFD[\x805g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x08\xEAW__\xFD[\x89` \x82\x84\x01\x01\x11\x15a\x08\xFBW__\xFD[` \x91\x90\x91\x01\x95P\x93P`@\x87\x015\x92Pa\t\x18``\x88\x01a\x08\rV[\x95\x98\x94\x97P\x92\x95\x91\x94\x93`\x80\x90\x92\x015\x92PPV[_` \x82\x84\x03\x12\x15a\t=W__\xFD[PQ\x91\x90PV[\x81\x83\x827_\x91\x01\x90\x81R\x91\x90PV\xFE\xA2dipfsX\"\x12 \xCE\x05<\x99\x96%\x12MY*F\xE9\xD9\xEFM\x9C%\xF3f\xA1a*\xC0}\xE5T\xCD\r]'\x0F\xD1dsolcC\0\x08\x1C\x003\xD5e\xE3\xFC\x06m\xF3H\xA5\xCB\xC0Z\x8Dc#\xE0\x05R\x83\x80A\xCE\xA2\xD8L\xC5\x98v\xBA7s]\x97fpp\xC5N\xF1\x82\xB0\xF5\x85\x8B\x03K\xEA\xC1\xB6\xF3\x08\x9A\xA2\xD3\x18\x8B\xB1\xE8\x92\x9FO\xA9\xB9)",
    );
    /// The runtime bytecode of the contract, as deployed on the network.
    ///
    /// ```text
    ///0x60806040526004361061017b575f3560e01c8063715018a6116100cd578063bc3488c611610087578063e16ca89511610062578063e16ca89514610474578063eb2347fd14610493578063f2fde38b146104ca578063f5b541a6146104e9575f5ffd5b8063bc3488c614610423578063d547741f14610436578063d80513b514610455575f5ffd5b8063715018a61461038f578063848c9f82146103a35780638da5cb5b146103b657806391d14854146103d2578063a217fddf146103f1578063b64b2a8a14610404575f5ffd5b80632f2ff15d116101385780634a3fba0e116101135780634a3fba0e146102ff57806353070b55146103325780635e8b95d21461035157806366e93b8c14610370575f5ffd5b80632f2ff15d146102a25780632f3819e6146102c157806336568abe146102e0575f5ffd5b806301ffc9a71461017f5780630c3148f5146101b357806311c9a94d146101fe5780631b7cac5f1461021d578063248a9ca3146102505780632a33cf2e1461028d575b5f5ffd5b34801561018a575f5ffd5b5061019e61019936600461199b565b610509565b60405190151581526020015b60405180910390f35b3480156101be575f5ffd5b506101e67f000000000000000000000000000000000000000000000000000000000000000081565b6040516001600160a01b0390911681526020016101aa565b348015610209575f5ffd5b5061019e6102183660046119d6565b61053f565b348015610228575f5ffd5b506101e67f000000000000000000000000000000000000000000000000000000000000000081565b34801561025b575f5ffd5b5061027f61026a366004611a1e565b5f908152600160208190526040909120015490565b6040519081526020016101aa565b6102a061029b366004611a7a565b61064f565b005b3480156102ad575f5ffd5b506102a06102bc366004611b0d565b610985565b3480156102cc575f5ffd5b506102a06102db366004611b3b565b6109b0565b3480156102eb575f5ffd5b506102a06102fa366004611b0d565b610afe565b34801561030a575f5ffd5b5061027f7fd565e3fc066df348a5cbc05a8d6323e00552838041cea2d84cc59876ba37735d81565b34801561033d575f5ffd5b506102a061034c366004611b96565b610b36565b34801561035c575f5ffd5b506101e661036b366004611b0d565b610c82565b34801561037b575f5ffd5b506102a061038a366004611be6565b610cbf565b34801561039a575f5ffd5b506102a0610e0c565b6102a06103b1366004611d4a565b610e1f565b3480156103c1575f5ffd5b505f546001600160a01b03166101e6565b3480156103dd575f5ffd5b5061019e6103ec366004611b0d565b6110ad565b3480156103fc575f5ffd5b5061027f5f81565b34801561040f575f5ffd5b506102a061041e366004611dcd565b6110d7565b6102a0610431366004611df8565b611299565b348015610441575f5ffd5b506102a0610450366004611b0d565b611514565b348015610460575f5ffd5b506101e661046f366004611e6a565b611539565b34801561047f575f5ffd5b506101e661048e366004611e95565b611571565b34801561049e575f5ffd5b5061019e6104ad366004611ec9565b6001600160a01b03165f9081526006602052604090205460ff1690565b3480156104d5575f5ffd5b506102a06104e4366004611ec9565b6115a2565b3480156104f4575f5ffd5b5061027f5f51602061222e5f395f51905f5281565b5f6001600160e01b03198216637965db0b60e01b148061053957506301ffc9a760e01b6001600160e01b03198316145b92915050565b5f7fd565e3fc066df348a5cbc05a8d6323e00552838041cea2d84cc59876ba37735d61056a816115e4565b6001600160a01b0385163b1561059657600380546001600160a01b0319166001600160a01b0387161790555b6001600160a01b0384163b156105c257600480546001600160a01b0319166001600160a01b0386161790555b6001600160a01b0383163b156105ee57600580546001600160a01b0319166001600160a01b0385161790555b600354600454600554604080516001600160a01b039485168152928416602084015292168183015290517fff8a97fda7728495ee3a5c551af3495e4ba23cdda2ac138491eff294902122ee9181900360600190a1600191505b509392505050565b5f51602061222e5f395f51905f52610666816115e4565b6005546001600160a01b031661068f5760405163437f3ac360e01b815260040160405180910390fd5b4684036107455760055460405163618c776d60e11b81525f916001600160a01b03169063c318eeda906106c8908d908d90600401611f0c565b60a060405180830381865afa1580156106e3573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906107079190611f2a565b9050856001600160a01b031681604001516001600160a01b03161461073f5760405163523660f760e01b815260040160405180910390fd5b50610816565b600554604051637f99d7af60e01b81525f916001600160a01b031690637f99d7af90610777908d908d90600401611f0c565b5f60405180830381865afa158015610791573d5f5f3e3d5ffd5b505050506040513d5f823e601f3d908101601f191682016040526107b8919081019061202e565b9050856001600160a01b03168160a001516001600160a01b0316146107f05760405163523660f760e01b815260040160405180910390fd5b848160e0015114610814576040516397c91b6960e01b815260040160405180910390fd5b505b5f6108235f8787876115ee565b90505f6108507f000000000000000000000000000000000000000000000000000000000000000083611645565b604051633b7e75bb60e11b81529091506001600160a01b038216906376fceb7690610885905f908b908b908b9060040161213d565b5f604051808303815f87803b15801561089c575f5ffd5b505af11580156108ae573d5f5f3e3d5ffd5b50505050806001600160a01b031663e3f5c5523460055f9054906101000a90046001600160a01b03168e8e8e8e8b6040518863ffffffff1660e01b81526004016108fd96959493929190612164565b5f604051808303818588803b158015610914575f5ffd5b505af1158015610926573d5f5f3e3d5ffd5b5050604080518a81526001600160a01b038d81166020830152808b1695508c81169450861692507fd32efcefa11997ebea13e4c365a2a92eefc7b965a74ba151dadb02a3cd500637910160405180910390a45050505050505050505050565b5f82815260016020819052604090912001546109a0816115e4565b6109aa8383611651565b50505050565b5f6109bc8686866116c7565b90505f6109e97f000000000000000000000000000000000000000000000000000000000000000083611645565b604051636315a14760e11b815260048101899052602481018890526001600160a01b0387811660448301529192509082169063c62b428e906064015f604051808303815f87803b158015610a3b575f5ffd5b505af1158015610a4d573d5f5f3e3d5ffd5b5050604051631922fddd60e21b81526001600160a01b03878116600483015286811660248301528416925063648bf77491506044015f604051808303815f87803b158015610a99575f5ffd5b505af1158015610aab573d5f5f3e3d5ffd5b50506040516001600160a01b03868116825280881693508a92508416907f2113859f5b6983a70726932ab3518e5bf454f731fdb16d96d8ac870a5b2322419060200160405180910390a450505050505050565b6001600160a01b0381163314610b275760405163334bd91960e11b815260040160405180910390fd5b610b318282611715565b505050565b5f610b43855f5f876115ee565b90505f610b707f000000000000000000000000000000000000000000000000000000000000000083611645565b604051633b7e75bb60e11b81529091506001600160a01b038216906376fceb7690610ba59089905f9081908b9060040161213d565b5f604051808303815f87803b158015610bbc575f5ffd5b505af1158015610bce573d5f5f3e3d5ffd5b5050604051631922fddd60e21b81526001600160a01b03878116600483015286811660248301528416925063648bf77491506044015f604051808303815f87803b158015610c1a575f5ffd5b505af1158015610c2c573d5f5f3e3d5ffd5b50505050826001600160a01b0316846001600160a01b0316826001600160a01b03167f248ad0b6173546f5b68ca1ea8493871fb220561b577d7d5fdf7bf404c1c1f87960405160405180910390a4505050505050565b5f610cb87f0000000000000000000000000000000000000000000000000000000000000000610cb3855f5f876115ee565b611780565b9392505050565b5f610ccc5f8787876115ee565b90505f610cf97f000000000000000000000000000000000000000000000000000000000000000083611645565b604051633b7e75bb60e11b81529091506001600160a01b038216906376fceb7690610d2e905f908b908b908b9060040161213d565b5f604051808303815f87803b158015610d45575f5ffd5b505af1158015610d57573d5f5f3e3d5ffd5b5050604051631922fddd60e21b81526001600160a01b03878116600483015286811660248301528416925063648bf77491506044015f604051808303815f87803b158015610da3575f5ffd5b505af1158015610db5573d5f5f3e3d5ffd5b50505050826001600160a01b0316846001600160a01b0316826001600160a01b03167f248ad0b6173546f5b68ca1ea8493871fb220561b577d7d5fdf7bf404c1c1f87960405160405180910390a450505050505050565b610e146117e8565b610e1d5f611814565b565b5f51602061222e5f395f51905f52610e36816115e4565b6005546001600160a01b0316610e5f5760405163437f3ac360e01b815260040160405180910390fd5b600554604051637f99d7af60e01b81525f916001600160a01b031690637f99d7af90610e91908b908b90600401611f0c565b5f60405180830381865afa158015610eab573d5f5f3e3d5ffd5b505050506040513d5f823e601f3d908101601f19168201604052610ed2919081019061202e565b9050610ee1865f015185610c82565b6001600160a01b03168160a001516001600160a01b031614610f165760405163523660f760e01b815260040160405180910390fd5b61a4b18160e0015114610f3c576040516397c91b6960e01b815260040160405180910390fd5b5f610f4c875f01515f5f886115ee565b90505f610f797f000000000000000000000000000000000000000000000000000000000000000083611645565b8851604051633b7e75bb60e11b81529192506001600160a01b038316916376fceb7691610fae915f9081908c9060040161213d565b5f604051808303815f87803b158015610fc5575f5ffd5b505af1158015610fd7573d5f5f3e3d5ffd5b50505050806001600160a01b031663e3f5c5523460055f9054906101000a90046001600160a01b03168d8d8d604001518d8c6040518863ffffffff1660e01b815260040161102a96959493929190612164565b5f604051808303818588803b158015611041575f5ffd5b505af1158015611053573d5f5f3e3d5ffd5b50508a516040516001600160a01b038c81168252808c16955091935090851691507f0d580c10960bd9b30908da4edf8e5f5ba6b1fbaf2c32d28be175a92d4b9896f29060200160405180910390a450505050505050505050565b5f9182526001602090815260408084206001600160a01b0393909316845291905290205460ff1690565b5f51602061222e5f395f51905f526110ee816115e4565b6003546001600160a01b0316158061110f57506004546001600160a01b0316155b1561112d576040516389da714f60e01b815260040160405180910390fd5b5f61113d845f01515f5f866115ee565b90505f61116a7f000000000000000000000000000000000000000000000000000000000000000083611645565b8551604051633b7e75bb60e11b81529192506001600160a01b038316916376fceb769161119f915f9081908a9060040161213d565b5f604051808303815f87803b1580156111b6575f5ffd5b505af11580156111c8573d5f5f3e3d5ffd5b5050506001600160a01b038083165f8181526006602052604090819020805460ff19166001179055600480546003549251630cce2a2760e11b815293955063199c544e94611221948c94928216939290911691016121af565b5f604051808303815f87803b158015611238575f5ffd5b505af115801561124a573d5f5f3e3d5ffd5b50505050836001600160a01b0316855f0151826001600160a01b03167f7153fc1ba1d8a6784e0a3aa1113e319ebf6702ccbdcc83a126a5d23b2671b07460405160405180910390a45050505050565b5f51602061222e5f395f51905f526112b0816115e4565b6005546001600160a01b03166112d95760405163437f3ac360e01b815260040160405180910390fd5b600554604051637f99d7af60e01b81525f916001600160a01b031690637f99d7af9061130b908d908d90600401611f0c565b5f60405180830381865afa158015611325573d5f5f3e3d5ffd5b505050506040513d5f823e601f3d908101601f1916820160405261134c919081019061202e565b9050848160e0015114611372576040516397c91b6960e01b815260040160405180910390fd5b856113838b8b846101000151611863565b146113a15760405163523660f760e01b815260040160405180910390fd5b5f6113ad8787876116c7565b90505f6113da7f000000000000000000000000000000000000000000000000000000000000000083611645565b604051636315a14760e11b8152600481018a9052602481018990526001600160a01b0388811660448301529192509082169063c62b428e906064015f604051808303815f87803b15801561142c575f5ffd5b505af115801561143e573d5f5f3e3d5ffd5b50505050806001600160a01b031663e3f5c5523460055f9054906101000a90046001600160a01b03168f8f8f8f8c6040518863ffffffff1660e01b815260040161148d96959493929190612164565b5f604051808303818588803b1580156114a4575f5ffd5b505af11580156114b6573d5f5f3e3d5ffd5b5050604080518b81526001600160a01b038e81166020830152808c1695508d9450861692507f2159a9beac3a6bdf488be58a40cb4df3c7649082933a290f1d86a43ebba2d213910160405180910390a4505050505050505050505050565b5f828152600160208190526040909120015461152f816115e4565b6109aa8383611715565b5f6115697f0000000000000000000000000000000000000000000000000000000000000000610cb38686866116c7565b949350505050565b5f6115697f0000000000000000000000000000000000000000000000000000000000000000610cb35f8787876115ee565b6115aa6117e8565b6001600160a01b0381166115d857604051631e4fbdf760e01b81525f60048201526024015b60405180910390fd5b6115e181611814565b50565b6115e181336118c9565b60025460408051602081019290925281018590526001600160a01b03808516606083015260808201849052821660a08201525f9060c001604051602081830303815290604052805190602001209050949350505050565b5f610cb883835f611906565b5f61165c83836110ad565b6116c0575f8381526001602081815260408084206001600160a01b0387168086529252808420805460ff19169093179092559051339286917f2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d9190a4506001610539565b505f610539565b6002546040805160208101929092528101849052606081018390526001600160a01b03821660808201525f9060a0016040516020818303038152906040528051906020012090509392505050565b5f61172083836110ad565b156116c0575f8381526001602090815260408083206001600160a01b0386168085529252808320805460ff1916905551339286917ff6391f5c32d9c69d2a47ea670b442974b53935d1edc7fd64eb21e047a839171b9190a4506001610539565b6040513060388201526f5af43d82803e903d91602b57fd5bf3ff602482015260148101839052733d602d80600a3d3981f3363d3d373d3d3d363d738152605881018290526037600c820120607882015260556043909101205f906001600160a01b0316610cb8565b5f546001600160a01b03163314610e1d5760405163118cdaa760e01b81523360048201526024016115cf565b5f80546001600160a01b038381166001600160a01b0319831681178455604051919092169283917f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e09190a35050565b5f5f84848080601f0160208091040260200160405190810160405280939291908181526020018383808284375f9201919091525092935050841591506118b790505760648101518101602401519150610647565b60448101510160240151949350505050565b6118d382826110ad565b6119025760405163e2517d3f60e01b81526001600160a01b0382166004820152602481018390526044016115cf565b5050565b5f814710156119315760405163cf47918160e01b8152476004820152602481018390526044016115cf565b763d602d80600a3d3981f3363d3d373d3d3d363d730000008460601b60e81c175f526e5af43d82803e903d91602b57fd5bf38460781b17602052826037600984f590506001600160a01b038116610cb85760405163b06ebf3d60e01b815260040160405180910390fd5b5f602082840312156119ab575f5ffd5b81356001600160e01b031981168114610cb8575f5ffd5b6001600160a01b03811681146115e1575f5ffd5b5f5f5f606084860312156119e8575f5ffd5b83356119f3816119c2565b92506020840135611a03816119c2565b91506040840135611a13816119c2565b809150509250925092565b5f60208284031215611a2e575f5ffd5b5035919050565b5f5f83601f840112611a45575f5ffd5b50813567ffffffffffffffff811115611a5c575f5ffd5b602083019150836020828501011115611a73575f5ffd5b9250929050565b5f5f5f5f5f5f5f5f60e0898b031215611a91575f5ffd5b883567ffffffffffffffff811115611aa7575f5ffd5b611ab38b828c01611a35565b909950975050602089013595506040890135611ace816119c2565b94506060890135611ade816119c2565b93506080890135925060a0890135611af5816119c2565b979a969950949793969295919450919260c001359150565b5f5f60408385031215611b1e575f5ffd5b823591506020830135611b30816119c2565b809150509250929050565b5f5f5f5f5f60a08688031215611b4f575f5ffd5b85359450602086013593506040860135611b68816119c2565b92506060860135611b78816119c2565b91506080860135611b88816119c2565b809150509295509295909350565b5f5f5f5f60808587031215611ba9575f5ffd5b843593506020850135611bbb816119c2565b92506040850135611bcb816119c2565b91506060850135611bdb816119c2565b939692955090935050565b5f5f5f5f5f60a08688031215611bfa575f5ffd5b8535611c05816119c2565b9450602086013593506040860135611b68816119c2565b634e487b7160e01b5f52604160045260245ffd5b60405160a0810167ffffffffffffffff81118282101715611c5357611c53611c1c565b60405290565b6040805190810167ffffffffffffffff81118282101715611c5357611c53611c1c565b604051610140810167ffffffffffffffff81118282101715611c5357611c53611c1c565b803561ffff81168114611cb1575f5ffd5b919050565b5f60c08284031215611cc6575f5ffd5b611cce611c30565b8235815260208084013590820152604080840135908201529050607f82018313611cf6575f5ffd5b611cfe611c59565b8060a0840185811115611d0f575f5ffd5b606085015b81811015611d2c578035845260209384019301611d14565b50816060850152611d3c81611ca0565b608085015250505092915050565b5f5f5f5f5f5f6101408789031215611d60575f5ffd5b863567ffffffffffffffff811115611d76575f5ffd5b611d8289828a01611a35565b9097509550611d9690508860208901611cb6565b935060e0870135611da6816119c2565b9250610100870135611db7816119c2565b9598949750929591949361012090920135925050565b5f5f60e08385031215611dde575f5ffd5b611de88484611cb6565b915060c0830135611b30816119c2565b5f5f5f5f5f5f5f5f60e0898b031215611e0f575f5ffd5b883567ffffffffffffffff811115611e25575f5ffd5b611e318b828c01611a35565b909950975050602089013595506040890135611e4c816119c2565b9450606089013593506080890135925060a0890135611af5816119c2565b5f5f5f60608486031215611e7c575f5ffd5b83359250602084013591506040840135611a13816119c2565b5f5f5f60608486031215611ea7575f5ffd5b8335611eb2816119c2565b9250602084013591506040840135611a13816119c2565b5f60208284031215611ed9575f5ffd5b8135610cb8816119c2565b81835281816020850137505f828201602090810191909152601f909101601f19169091010190565b602081525f611569602083018486611ee4565b8051611cb1816119c2565b5f60a0828403128015611f3b575f5ffd5b50611f44611c30565b8251611f4f816119c2565b8152602083810151908201526040830151611f69816119c2565b60408201526060830151611f7c816119c2565b60608201526080928301519281019290925250919050565b5f82601f830112611fa3575f5ffd5b815167ffffffffffffffff811115611fbd57611fbd611c1c565b604051601f8201601f19908116603f0116810167ffffffffffffffff81118282101715611fec57611fec611c1c565b604052818152838201602001851015612003575f5ffd5b8160208501602083015e5f918101602001919091529392505050565b80518015158114611cb1575f5ffd5b5f6020828403121561203e575f5ffd5b815167ffffffffffffffff811115612054575f5ffd5b82016101408185031215612066575f5ffd5b61206e611c7c565b81518152602082015167ffffffffffffffff81111561208b575f5ffd5b61209786828501611f94565b602083015250604082015167ffffffffffffffff8111156120b6575f5ffd5b6120c286828501611f94565b6040830152506120d460608301611f1f565b60608201526120e560808301611f1f565b60808201526120f660a08301611f1f565b60a082015260c0828101519082015260e0808301519082015261211c610100830161201f565b61010082015261212f610120830161201f565b610120820152949350505050565b9384526001600160a01b039283166020850152604084019190915216606082015260800190565b6001600160a01b038716815260a0602082018190525f906121889083018789611ee4565b6040830195909552506001600160a01b039290921660608301526080909101529392505050565b5f610100820190508451825260208501516020830152604085015160408301526060850151606083015f5b60028110156121f95782518252602092830192909101906001016121da565b505050608085015161ffff1660a08301526001600160a01b03841660c08301526001600160a01b03831660e083015261156956fe97667070c54ef182b0f5858b034beac1b6f3089aa2d3188bb1e8929f4fa9b929a26469706673582212201492fef915baf9e19fe3e53e36603890943cf588d0d779f0c71db4183307576a64736f6c634300081c0033
    /// ```
    #[rustfmt::skip]
    #[allow(clippy::all)]
    pub static DEPLOYED_BYTECODE: alloy_sol_types::private::Bytes = alloy_sol_types::private::Bytes::from_static(
        b"`\x80`@R`\x046\x10a\x01{W_5`\xE0\x1C\x80cqP\x18\xA6\x11a\0\xCDW\x80c\xBC4\x88\xC6\x11a\0\x87W\x80c\xE1l\xA8\x95\x11a\0bW\x80c\xE1l\xA8\x95\x14a\x04tW\x80c\xEB#G\xFD\x14a\x04\x93W\x80c\xF2\xFD\xE3\x8B\x14a\x04\xCAW\x80c\xF5\xB5A\xA6\x14a\x04\xE9W__\xFD[\x80c\xBC4\x88\xC6\x14a\x04#W\x80c\xD5Gt\x1F\x14a\x046W\x80c\xD8\x05\x13\xB5\x14a\x04UW__\xFD[\x80cqP\x18\xA6\x14a\x03\x8FW\x80c\x84\x8C\x9F\x82\x14a\x03\xA3W\x80c\x8D\xA5\xCB[\x14a\x03\xB6W\x80c\x91\xD1HT\x14a\x03\xD2W\x80c\xA2\x17\xFD\xDF\x14a\x03\xF1W\x80c\xB6K*\x8A\x14a\x04\x04W__\xFD[\x80c//\xF1]\x11a\x018W\x80cJ?\xBA\x0E\x11a\x01\x13W\x80cJ?\xBA\x0E\x14a\x02\xFFW\x80cS\x07\x0BU\x14a\x032W\x80c^\x8B\x95\xD2\x14a\x03QW\x80cf\xE9;\x8C\x14a\x03pW__\xFD[\x80c//\xF1]\x14a\x02\xA2W\x80c/8\x19\xE6\x14a\x02\xC1W\x80c6V\x8A\xBE\x14a\x02\xE0W__\xFD[\x80c\x01\xFF\xC9\xA7\x14a\x01\x7FW\x80c\x0C1H\xF5\x14a\x01\xB3W\x80c\x11\xC9\xA9M\x14a\x01\xFEW\x80c\x1B|\xAC_\x14a\x02\x1DW\x80c$\x8A\x9C\xA3\x14a\x02PW\x80c*3\xCF.\x14a\x02\x8DW[__\xFD[4\x80\x15a\x01\x8AW__\xFD[Pa\x01\x9Ea\x01\x996`\x04a\x19\x9BV[a\x05\tV[`@Q\x90\x15\x15\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[4\x80\x15a\x01\xBEW__\xFD[Pa\x01\xE6\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81V[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01a\x01\xAAV[4\x80\x15a\x02\tW__\xFD[Pa\x01\x9Ea\x02\x186`\x04a\x19\xD6V[a\x05?V[4\x80\x15a\x02(W__\xFD[Pa\x01\xE6\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81V[4\x80\x15a\x02[W__\xFD[Pa\x02\x7Fa\x02j6`\x04a\x1A\x1EV[_\x90\x81R`\x01` \x81\x90R`@\x90\x91 \x01T\x90V[`@Q\x90\x81R` \x01a\x01\xAAV[a\x02\xA0a\x02\x9B6`\x04a\x1AzV[a\x06OV[\0[4\x80\x15a\x02\xADW__\xFD[Pa\x02\xA0a\x02\xBC6`\x04a\x1B\rV[a\t\x85V[4\x80\x15a\x02\xCCW__\xFD[Pa\x02\xA0a\x02\xDB6`\x04a\x1B;V[a\t\xB0V[4\x80\x15a\x02\xEBW__\xFD[Pa\x02\xA0a\x02\xFA6`\x04a\x1B\rV[a\n\xFEV[4\x80\x15a\x03\nW__\xFD[Pa\x02\x7F\x7F\xD5e\xE3\xFC\x06m\xF3H\xA5\xCB\xC0Z\x8Dc#\xE0\x05R\x83\x80A\xCE\xA2\xD8L\xC5\x98v\xBA7s]\x81V[4\x80\x15a\x03=W__\xFD[Pa\x02\xA0a\x03L6`\x04a\x1B\x96V[a\x0B6V[4\x80\x15a\x03\\W__\xFD[Pa\x01\xE6a\x03k6`\x04a\x1B\rV[a\x0C\x82V[4\x80\x15a\x03{W__\xFD[Pa\x02\xA0a\x03\x8A6`\x04a\x1B\xE6V[a\x0C\xBFV[4\x80\x15a\x03\x9AW__\xFD[Pa\x02\xA0a\x0E\x0CV[a\x02\xA0a\x03\xB16`\x04a\x1DJV[a\x0E\x1FV[4\x80\x15a\x03\xC1W__\xFD[P_T`\x01`\x01`\xA0\x1B\x03\x16a\x01\xE6V[4\x80\x15a\x03\xDDW__\xFD[Pa\x01\x9Ea\x03\xEC6`\x04a\x1B\rV[a\x10\xADV[4\x80\x15a\x03\xFCW__\xFD[Pa\x02\x7F_\x81V[4\x80\x15a\x04\x0FW__\xFD[Pa\x02\xA0a\x04\x1E6`\x04a\x1D\xCDV[a\x10\xD7V[a\x02\xA0a\x0416`\x04a\x1D\xF8V[a\x12\x99V[4\x80\x15a\x04AW__\xFD[Pa\x02\xA0a\x04P6`\x04a\x1B\rV[a\x15\x14V[4\x80\x15a\x04`W__\xFD[Pa\x01\xE6a\x04o6`\x04a\x1EjV[a\x159V[4\x80\x15a\x04\x7FW__\xFD[Pa\x01\xE6a\x04\x8E6`\x04a\x1E\x95V[a\x15qV[4\x80\x15a\x04\x9EW__\xFD[Pa\x01\x9Ea\x04\xAD6`\x04a\x1E\xC9V[`\x01`\x01`\xA0\x1B\x03\x16_\x90\x81R`\x06` R`@\x90 T`\xFF\x16\x90V[4\x80\x15a\x04\xD5W__\xFD[Pa\x02\xA0a\x04\xE46`\x04a\x1E\xC9V[a\x15\xA2V[4\x80\x15a\x04\xF4W__\xFD[Pa\x02\x7F_Q` a\"._9_Q\x90_R\x81V[_`\x01`\x01`\xE0\x1B\x03\x19\x82\x16cye\xDB\x0B`\xE0\x1B\x14\x80a\x059WPc\x01\xFF\xC9\xA7`\xE0\x1B`\x01`\x01`\xE0\x1B\x03\x19\x83\x16\x14[\x92\x91PPV[_\x7F\xD5e\xE3\xFC\x06m\xF3H\xA5\xCB\xC0Z\x8Dc#\xE0\x05R\x83\x80A\xCE\xA2\xD8L\xC5\x98v\xBA7s]a\x05j\x81a\x15\xE4V[`\x01`\x01`\xA0\x1B\x03\x85\x16;\x15a\x05\x96W`\x03\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x87\x16\x17\x90U[`\x01`\x01`\xA0\x1B\x03\x84\x16;\x15a\x05\xC2W`\x04\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x86\x16\x17\x90U[`\x01`\x01`\xA0\x1B\x03\x83\x16;\x15a\x05\xEEW`\x05\x80T`\x01`\x01`\xA0\x1B\x03\x19\x16`\x01`\x01`\xA0\x1B\x03\x85\x16\x17\x90U[`\x03T`\x04T`\x05T`@\x80Q`\x01`\x01`\xA0\x1B\x03\x94\x85\x16\x81R\x92\x84\x16` \x84\x01R\x92\x16\x81\x83\x01R\x90Q\x7F\xFF\x8A\x97\xFD\xA7r\x84\x95\xEE:\\U\x1A\xF3I^K\xA2<\xDD\xA2\xAC\x13\x84\x91\xEF\xF2\x94\x90!\"\xEE\x91\x81\x90\x03``\x01\x90\xA1`\x01\x91P[P\x93\x92PPPV[_Q` a\"._9_Q\x90_Ra\x06f\x81a\x15\xE4V[`\x05T`\x01`\x01`\xA0\x1B\x03\x16a\x06\x8FW`@QcC\x7F:\xC3`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[F\x84\x03a\x07EW`\x05T`@Qca\x8Cwm`\xE1\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\xC3\x18\xEE\xDA\x90a\x06\xC8\x90\x8D\x90\x8D\x90`\x04\x01a\x1F\x0CV[`\xA0`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x06\xE3W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x07\x07\x91\x90a\x1F*V[\x90P\x85`\x01`\x01`\xA0\x1B\x03\x16\x81`@\x01Q`\x01`\x01`\xA0\x1B\x03\x16\x14a\x07?W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[Pa\x08\x16V[`\x05T`@Qc\x7F\x99\xD7\xAF`\xE0\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\x7F\x99\xD7\xAF\x90a\x07w\x90\x8D\x90\x8D\x90`\x04\x01a\x1F\x0CV[_`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x07\x91W=__>=_\xFD[PPPP`@Q=_\x82>`\x1F=\x90\x81\x01`\x1F\x19\x16\x82\x01`@Ra\x07\xB8\x91\x90\x81\x01\x90a .V[\x90P\x85`\x01`\x01`\xA0\x1B\x03\x16\x81`\xA0\x01Q`\x01`\x01`\xA0\x1B\x03\x16\x14a\x07\xF0W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x84\x81`\xE0\x01Q\x14a\x08\x14W`@Qc\x97\xC9\x1Bi`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[P[_a\x08#_\x87\x87\x87a\x15\xEEV[\x90P_a\x08P\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qc;~u\xBB`\xE1\x1B\x81R\x90\x91P`\x01`\x01`\xA0\x1B\x03\x82\x16\x90cv\xFC\xEBv\x90a\x08\x85\x90_\x90\x8B\x90\x8B\x90\x8B\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x08\x9CW__\xFD[PZ\xF1\x15\x80\x15a\x08\xAEW=__>=_\xFD[PPPP\x80`\x01`\x01`\xA0\x1B\x03\x16c\xE3\xF5\xC5R4`\x05_\x90T\x90a\x01\0\n\x90\x04`\x01`\x01`\xA0\x1B\x03\x16\x8E\x8E\x8E\x8E\x8B`@Q\x88c\xFF\xFF\xFF\xFF\x16`\xE0\x1B\x81R`\x04\x01a\x08\xFD\x96\x95\x94\x93\x92\x91\x90a!dV[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\t\x14W__\xFD[PZ\xF1\x15\x80\x15a\t&W=__>=_\xFD[PP`@\x80Q\x8A\x81R`\x01`\x01`\xA0\x1B\x03\x8D\x81\x16` \x83\x01R\x80\x8B\x16\x95P\x8C\x81\x16\x94P\x86\x16\x92P\x7F\xD3.\xFC\xEF\xA1\x19\x97\xEB\xEA\x13\xE4\xC3e\xA2\xA9.\xEF\xC7\xB9e\xA7K\xA1Q\xDA\xDB\x02\xA3\xCDP\x067\x91\x01`@Q\x80\x91\x03\x90\xA4PPPPPPPPPPPV[_\x82\x81R`\x01` \x81\x90R`@\x90\x91 \x01Ta\t\xA0\x81a\x15\xE4V[a\t\xAA\x83\x83a\x16QV[PPPPV[_a\t\xBC\x86\x86\x86a\x16\xC7V[\x90P_a\t\xE9\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qcc\x15\xA1G`\xE1\x1B\x81R`\x04\x81\x01\x89\x90R`$\x81\x01\x88\x90R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`D\x83\x01R\x91\x92P\x90\x82\x16\x90c\xC6+B\x8E\x90`d\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\n;W__\xFD[PZ\xF1\x15\x80\x15a\nMW=__>=_\xFD[PP`@Qc\x19\"\xFD\xDD`\xE2\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`\x04\x83\x01R\x86\x81\x16`$\x83\x01R\x84\x16\x92Pcd\x8B\xF7t\x91P`D\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\n\x99W__\xFD[PZ\xF1\x15\x80\x15a\n\xABW=__>=_\xFD[PP`@Q`\x01`\x01`\xA0\x1B\x03\x86\x81\x16\x82R\x80\x88\x16\x93P\x8A\x92P\x84\x16\x90\x7F!\x13\x85\x9F[i\x83\xA7\x07&\x93*\xB3Q\x8E[\xF4T\xF71\xFD\xB1m\x96\xD8\xAC\x87\n[#\"A\x90` \x01`@Q\x80\x91\x03\x90\xA4PPPPPPPV[`\x01`\x01`\xA0\x1B\x03\x81\x163\x14a\x0B'W`@Qc3K\xD9\x19`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x0B1\x82\x82a\x17\x15V[PPPV[_a\x0BC\x85__\x87a\x15\xEEV[\x90P_a\x0Bp\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qc;~u\xBB`\xE1\x1B\x81R\x90\x91P`\x01`\x01`\xA0\x1B\x03\x82\x16\x90cv\xFC\xEBv\x90a\x0B\xA5\x90\x89\x90_\x90\x81\x90\x8B\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x0B\xBCW__\xFD[PZ\xF1\x15\x80\x15a\x0B\xCEW=__>=_\xFD[PP`@Qc\x19\"\xFD\xDD`\xE2\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`\x04\x83\x01R\x86\x81\x16`$\x83\x01R\x84\x16\x92Pcd\x8B\xF7t\x91P`D\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x0C\x1AW__\xFD[PZ\xF1\x15\x80\x15a\x0C,W=__>=_\xFD[PPPP\x82`\x01`\x01`\xA0\x1B\x03\x16\x84`\x01`\x01`\xA0\x1B\x03\x16\x82`\x01`\x01`\xA0\x1B\x03\x16\x7F$\x8A\xD0\xB6\x175F\xF5\xB6\x8C\xA1\xEA\x84\x93\x87\x1F\xB2 V\x1BW}}_\xDF{\xF4\x04\xC1\xC1\xF8y`@Q`@Q\x80\x91\x03\x90\xA4PPPPPPV[_a\x0C\xB8\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x0C\xB3\x85__\x87a\x15\xEEV[a\x17\x80V[\x93\x92PPPV[_a\x0C\xCC_\x87\x87\x87a\x15\xEEV[\x90P_a\x0C\xF9\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qc;~u\xBB`\xE1\x1B\x81R\x90\x91P`\x01`\x01`\xA0\x1B\x03\x82\x16\x90cv\xFC\xEBv\x90a\r.\x90_\x90\x8B\x90\x8B\x90\x8B\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\rEW__\xFD[PZ\xF1\x15\x80\x15a\rWW=__>=_\xFD[PP`@Qc\x19\"\xFD\xDD`\xE2\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x87\x81\x16`\x04\x83\x01R\x86\x81\x16`$\x83\x01R\x84\x16\x92Pcd\x8B\xF7t\x91P`D\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\r\xA3W__\xFD[PZ\xF1\x15\x80\x15a\r\xB5W=__>=_\xFD[PPPP\x82`\x01`\x01`\xA0\x1B\x03\x16\x84`\x01`\x01`\xA0\x1B\x03\x16\x82`\x01`\x01`\xA0\x1B\x03\x16\x7F$\x8A\xD0\xB6\x175F\xF5\xB6\x8C\xA1\xEA\x84\x93\x87\x1F\xB2 V\x1BW}}_\xDF{\xF4\x04\xC1\xC1\xF8y`@Q`@Q\x80\x91\x03\x90\xA4PPPPPPPV[a\x0E\x14a\x17\xE8V[a\x0E\x1D_a\x18\x14V[V[_Q` a\"._9_Q\x90_Ra\x0E6\x81a\x15\xE4V[`\x05T`\x01`\x01`\xA0\x1B\x03\x16a\x0E_W`@QcC\x7F:\xC3`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05T`@Qc\x7F\x99\xD7\xAF`\xE0\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\x7F\x99\xD7\xAF\x90a\x0E\x91\x90\x8B\x90\x8B\x90`\x04\x01a\x1F\x0CV[_`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x0E\xABW=__>=_\xFD[PPPP`@Q=_\x82>`\x1F=\x90\x81\x01`\x1F\x19\x16\x82\x01`@Ra\x0E\xD2\x91\x90\x81\x01\x90a .V[\x90Pa\x0E\xE1\x86_\x01Q\x85a\x0C\x82V[`\x01`\x01`\xA0\x1B\x03\x16\x81`\xA0\x01Q`\x01`\x01`\xA0\x1B\x03\x16\x14a\x0F\x16W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\xA4\xB1\x81`\xE0\x01Q\x14a\x0F<W`@Qc\x97\xC9\x1Bi`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_a\x0FL\x87_\x01Q__\x88a\x15\xEEV[\x90P_a\x0Fy\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[\x88Q`@Qc;~u\xBB`\xE1\x1B\x81R\x91\x92P`\x01`\x01`\xA0\x1B\x03\x83\x16\x91cv\xFC\xEBv\x91a\x0F\xAE\x91_\x90\x81\x90\x8C\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x0F\xC5W__\xFD[PZ\xF1\x15\x80\x15a\x0F\xD7W=__>=_\xFD[PPPP\x80`\x01`\x01`\xA0\x1B\x03\x16c\xE3\xF5\xC5R4`\x05_\x90T\x90a\x01\0\n\x90\x04`\x01`\x01`\xA0\x1B\x03\x16\x8D\x8D\x8D`@\x01Q\x8D\x8C`@Q\x88c\xFF\xFF\xFF\xFF\x16`\xE0\x1B\x81R`\x04\x01a\x10*\x96\x95\x94\x93\x92\x91\x90a!dV[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x10AW__\xFD[PZ\xF1\x15\x80\x15a\x10SW=__>=_\xFD[PP\x8AQ`@Q`\x01`\x01`\xA0\x1B\x03\x8C\x81\x16\x82R\x80\x8C\x16\x95P\x91\x93P\x90\x85\x16\x91P\x7F\rX\x0C\x10\x96\x0B\xD9\xB3\t\x08\xDAN\xDF\x8E_[\xA6\xB1\xFB\xAF,2\xD2\x8B\xE1u\xA9-K\x98\x96\xF2\x90` \x01`@Q\x80\x91\x03\x90\xA4PPPPPPPPPPV[_\x91\x82R`\x01` \x90\x81R`@\x80\x84 `\x01`\x01`\xA0\x1B\x03\x93\x90\x93\x16\x84R\x91\x90R\x90 T`\xFF\x16\x90V[_Q` a\"._9_Q\x90_Ra\x10\xEE\x81a\x15\xE4V[`\x03T`\x01`\x01`\xA0\x1B\x03\x16\x15\x80a\x11\x0FWP`\x04T`\x01`\x01`\xA0\x1B\x03\x16\x15[\x15a\x11-W`@Qc\x89\xDAqO`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_a\x11=\x84_\x01Q__\x86a\x15\xEEV[\x90P_a\x11j\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[\x85Q`@Qc;~u\xBB`\xE1\x1B\x81R\x91\x92P`\x01`\x01`\xA0\x1B\x03\x83\x16\x91cv\xFC\xEBv\x91a\x11\x9F\x91_\x90\x81\x90\x8A\x90`\x04\x01a!=V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x11\xB6W__\xFD[PZ\xF1\x15\x80\x15a\x11\xC8W=__>=_\xFD[PPP`\x01`\x01`\xA0\x1B\x03\x80\x83\x16_\x81\x81R`\x06` R`@\x90\x81\x90 \x80T`\xFF\x19\x16`\x01\x17\x90U`\x04\x80T`\x03T\x92Qc\x0C\xCE*'`\xE1\x1B\x81R\x93\x95Pc\x19\x9CTN\x94a\x12!\x94\x8C\x94\x92\x82\x16\x93\x92\x90\x91\x16\x91\x01a!\xAFV[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x128W__\xFD[PZ\xF1\x15\x80\x15a\x12JW=__>=_\xFD[PPPP\x83`\x01`\x01`\xA0\x1B\x03\x16\x85_\x01Q\x82`\x01`\x01`\xA0\x1B\x03\x16\x7FqS\xFC\x1B\xA1\xD8\xA6xN\n:\xA1\x11>1\x9E\xBFg\x02\xCC\xBD\xCC\x83\xA1&\xA5\xD2;&q\xB0t`@Q`@Q\x80\x91\x03\x90\xA4PPPPPV[_Q` a\"._9_Q\x90_Ra\x12\xB0\x81a\x15\xE4V[`\x05T`\x01`\x01`\xA0\x1B\x03\x16a\x12\xD9W`@QcC\x7F:\xC3`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05T`@Qc\x7F\x99\xD7\xAF`\xE0\x1B\x81R_\x91`\x01`\x01`\xA0\x1B\x03\x16\x90c\x7F\x99\xD7\xAF\x90a\x13\x0B\x90\x8D\x90\x8D\x90`\x04\x01a\x1F\x0CV[_`@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x13%W=__>=_\xFD[PPPP`@Q=_\x82>`\x1F=\x90\x81\x01`\x1F\x19\x16\x82\x01`@Ra\x13L\x91\x90\x81\x01\x90a .V[\x90P\x84\x81`\xE0\x01Q\x14a\x13rW`@Qc\x97\xC9\x1Bi`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x85a\x13\x83\x8B\x8B\x84a\x01\0\x01Qa\x18cV[\x14a\x13\xA1W`@QcR6`\xF7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_a\x13\xAD\x87\x87\x87a\x16\xC7V[\x90P_a\x13\xDA\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x83a\x16EV[`@Qcc\x15\xA1G`\xE1\x1B\x81R`\x04\x81\x01\x8A\x90R`$\x81\x01\x89\x90R`\x01`\x01`\xA0\x1B\x03\x88\x81\x16`D\x83\x01R\x91\x92P\x90\x82\x16\x90c\xC6+B\x8E\x90`d\x01_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x14,W__\xFD[PZ\xF1\x15\x80\x15a\x14>W=__>=_\xFD[PPPP\x80`\x01`\x01`\xA0\x1B\x03\x16c\xE3\xF5\xC5R4`\x05_\x90T\x90a\x01\0\n\x90\x04`\x01`\x01`\xA0\x1B\x03\x16\x8F\x8F\x8F\x8F\x8C`@Q\x88c\xFF\xFF\xFF\xFF\x16`\xE0\x1B\x81R`\x04\x01a\x14\x8D\x96\x95\x94\x93\x92\x91\x90a!dV[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x14\xA4W__\xFD[PZ\xF1\x15\x80\x15a\x14\xB6W=__>=_\xFD[PP`@\x80Q\x8B\x81R`\x01`\x01`\xA0\x1B\x03\x8E\x81\x16` \x83\x01R\x80\x8C\x16\x95P\x8D\x94P\x86\x16\x92P\x7F!Y\xA9\xBE\xAC:k\xDFH\x8B\xE5\x8A@\xCBM\xF3\xC7d\x90\x82\x93:)\x0F\x1D\x86\xA4>\xBB\xA2\xD2\x13\x91\x01`@Q\x80\x91\x03\x90\xA4PPPPPPPPPPPPV[_\x82\x81R`\x01` \x81\x90R`@\x90\x91 \x01Ta\x15/\x81a\x15\xE4V[a\t\xAA\x83\x83a\x17\x15V[_a\x15i\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x0C\xB3\x86\x86\x86a\x16\xC7V[\x94\x93PPPPV[_a\x15i\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x0C\xB3_\x87\x87\x87a\x15\xEEV[a\x15\xAAa\x17\xE8V[`\x01`\x01`\xA0\x1B\x03\x81\x16a\x15\xD8W`@Qc\x1EO\xBD\xF7`\xE0\x1B\x81R_`\x04\x82\x01R`$\x01[`@Q\x80\x91\x03\x90\xFD[a\x15\xE1\x81a\x18\x14V[PV[a\x15\xE1\x813a\x18\xC9V[`\x02T`@\x80Q` \x81\x01\x92\x90\x92R\x81\x01\x85\x90R`\x01`\x01`\xA0\x1B\x03\x80\x85\x16``\x83\x01R`\x80\x82\x01\x84\x90R\x82\x16`\xA0\x82\x01R_\x90`\xC0\x01`@Q` \x81\x83\x03\x03\x81R\x90`@R\x80Q\x90` \x01 \x90P\x94\x93PPPPV[_a\x0C\xB8\x83\x83_a\x19\x06V[_a\x16\\\x83\x83a\x10\xADV[a\x16\xC0W_\x83\x81R`\x01` \x81\x81R`@\x80\x84 `\x01`\x01`\xA0\x1B\x03\x87\x16\x80\x86R\x92R\x80\x84 \x80T`\xFF\x19\x16\x90\x93\x17\x90\x92U\x90Q3\x92\x86\x91\x7F/\x87\x88\x11~~\xFF\x1D\x82\xE9&\xECyI\x01\xD1|x\x02JP'\t@0E@\xA73eo\r\x91\x90\xA4P`\x01a\x059V[P_a\x059V[`\x02T`@\x80Q` \x81\x01\x92\x90\x92R\x81\x01\x84\x90R``\x81\x01\x83\x90R`\x01`\x01`\xA0\x1B\x03\x82\x16`\x80\x82\x01R_\x90`\xA0\x01`@Q` \x81\x83\x03\x03\x81R\x90`@R\x80Q\x90` \x01 \x90P\x93\x92PPPV[_a\x17 \x83\x83a\x10\xADV[\x15a\x16\xC0W_\x83\x81R`\x01` \x90\x81R`@\x80\x83 `\x01`\x01`\xA0\x1B\x03\x86\x16\x80\x85R\x92R\x80\x83 \x80T`\xFF\x19\x16\x90UQ3\x92\x86\x91\x7F\xF69\x1F\\2\xD9\xC6\x9D*G\xEAg\x0BD)t\xB595\xD1\xED\xC7\xFDd\xEB!\xE0G\xA89\x17\x1B\x91\x90\xA4P`\x01a\x059V[`@Q0`8\x82\x01RoZ\xF4=\x82\x80>\x90=\x91`+W\xFD[\xF3\xFF`$\x82\x01R`\x14\x81\x01\x83\x90Rs=`-\x80`\n=9\x81\xF36==7===6=s\x81R`X\x81\x01\x82\x90R`7`\x0C\x82\x01 `x\x82\x01R`U`C\x90\x91\x01 _\x90`\x01`\x01`\xA0\x1B\x03\x16a\x0C\xB8V[_T`\x01`\x01`\xA0\x1B\x03\x163\x14a\x0E\x1DW`@Qc\x11\x8C\xDA\xA7`\xE0\x1B\x81R3`\x04\x82\x01R`$\x01a\x15\xCFV[_\x80T`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`\x01`\x01`\xA0\x1B\x03\x19\x83\x16\x81\x17\x84U`@Q\x91\x90\x92\x16\x92\x83\x91\x7F\x8B\xE0\x07\x9CS\x16Y\x14\x13D\xCD\x1F\xD0\xA4\xF2\x84\x19I\x7F\x97\"\xA3\xDA\xAF\xE3\xB4\x18okdW\xE0\x91\x90\xA3PPV[__\x84\x84\x80\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x93\x92\x91\x90\x81\x81R` \x01\x83\x83\x80\x82\x847_\x92\x01\x91\x90\x91RP\x92\x93PP\x84\x15\x91Pa\x18\xB7\x90PW`d\x81\x01Q\x81\x01`$\x01Q\x91Pa\x06GV[`D\x81\x01Q\x01`$\x01Q\x94\x93PPPPV[a\x18\xD3\x82\x82a\x10\xADV[a\x19\x02W`@Qc\xE2Q}?`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x82\x16`\x04\x82\x01R`$\x81\x01\x83\x90R`D\x01a\x15\xCFV[PPV[_\x81G\x10\x15a\x191W`@Qc\xCFG\x91\x81`\xE0\x1B\x81RG`\x04\x82\x01R`$\x81\x01\x83\x90R`D\x01a\x15\xCFV[v=`-\x80`\n=9\x81\xF36==7===6=s\0\0\0\x84``\x1B`\xE8\x1C\x17_RnZ\xF4=\x82\x80>\x90=\x91`+W\xFD[\xF3\x84`x\x1B\x17` R\x82`7`\t\x84\xF5\x90P`\x01`\x01`\xA0\x1B\x03\x81\x16a\x0C\xB8W`@Qc\xB0n\xBF=`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_` \x82\x84\x03\x12\x15a\x19\xABW__\xFD[\x815`\x01`\x01`\xE0\x1B\x03\x19\x81\x16\x81\x14a\x0C\xB8W__\xFD[`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x15\xE1W__\xFD[___``\x84\x86\x03\x12\x15a\x19\xE8W__\xFD[\x835a\x19\xF3\x81a\x19\xC2V[\x92P` \x84\x015a\x1A\x03\x81a\x19\xC2V[\x91P`@\x84\x015a\x1A\x13\x81a\x19\xC2V[\x80\x91PP\x92P\x92P\x92V[_` \x82\x84\x03\x12\x15a\x1A.W__\xFD[P5\x91\x90PV[__\x83`\x1F\x84\x01\x12a\x1AEW__\xFD[P\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1A\\W__\xFD[` \x83\x01\x91P\x83` \x82\x85\x01\x01\x11\x15a\x1AsW__\xFD[\x92P\x92\x90PV[________`\xE0\x89\x8B\x03\x12\x15a\x1A\x91W__\xFD[\x885g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1A\xA7W__\xFD[a\x1A\xB3\x8B\x82\x8C\x01a\x1A5V[\x90\x99P\x97PP` \x89\x015\x95P`@\x89\x015a\x1A\xCE\x81a\x19\xC2V[\x94P``\x89\x015a\x1A\xDE\x81a\x19\xC2V[\x93P`\x80\x89\x015\x92P`\xA0\x89\x015a\x1A\xF5\x81a\x19\xC2V[\x97\x9A\x96\x99P\x94\x97\x93\x96\x92\x95\x91\x94P\x91\x92`\xC0\x015\x91PV[__`@\x83\x85\x03\x12\x15a\x1B\x1EW__\xFD[\x825\x91P` \x83\x015a\x1B0\x81a\x19\xC2V[\x80\x91PP\x92P\x92\x90PV[_____`\xA0\x86\x88\x03\x12\x15a\x1BOW__\xFD[\x855\x94P` \x86\x015\x93P`@\x86\x015a\x1Bh\x81a\x19\xC2V[\x92P``\x86\x015a\x1Bx\x81a\x19\xC2V[\x91P`\x80\x86\x015a\x1B\x88\x81a\x19\xC2V[\x80\x91PP\x92\x95P\x92\x95\x90\x93PV[____`\x80\x85\x87\x03\x12\x15a\x1B\xA9W__\xFD[\x845\x93P` \x85\x015a\x1B\xBB\x81a\x19\xC2V[\x92P`@\x85\x015a\x1B\xCB\x81a\x19\xC2V[\x91P``\x85\x015a\x1B\xDB\x81a\x19\xC2V[\x93\x96\x92\x95P\x90\x93PPV[_____`\xA0\x86\x88\x03\x12\x15a\x1B\xFAW__\xFD[\x855a\x1C\x05\x81a\x19\xC2V[\x94P` \x86\x015\x93P`@\x86\x015a\x1Bh\x81a\x19\xC2V[cNH{q`\xE0\x1B_R`A`\x04R`$_\xFD[`@Q`\xA0\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1CSWa\x1CSa\x1C\x1CV[`@R\x90V[`@\x80Q\x90\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1CSWa\x1CSa\x1C\x1CV[`@Qa\x01@\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1CSWa\x1CSa\x1C\x1CV[\x805a\xFF\xFF\x81\x16\x81\x14a\x1C\xB1W__\xFD[\x91\x90PV[_`\xC0\x82\x84\x03\x12\x15a\x1C\xC6W__\xFD[a\x1C\xCEa\x1C0V[\x825\x81R` \x80\x84\x015\x90\x82\x01R`@\x80\x84\x015\x90\x82\x01R\x90P`\x7F\x82\x01\x83\x13a\x1C\xF6W__\xFD[a\x1C\xFEa\x1CYV[\x80`\xA0\x84\x01\x85\x81\x11\x15a\x1D\x0FW__\xFD[``\x85\x01[\x81\x81\x10\x15a\x1D,W\x805\x84R` \x93\x84\x01\x93\x01a\x1D\x14V[P\x81``\x85\x01Ra\x1D<\x81a\x1C\xA0V[`\x80\x85\x01RPPP\x92\x91PPV[______a\x01@\x87\x89\x03\x12\x15a\x1D`W__\xFD[\x865g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1DvW__\xFD[a\x1D\x82\x89\x82\x8A\x01a\x1A5V[\x90\x97P\x95Pa\x1D\x96\x90P\x88` \x89\x01a\x1C\xB6V[\x93P`\xE0\x87\x015a\x1D\xA6\x81a\x19\xC2V[\x92Pa\x01\0\x87\x015a\x1D\xB7\x81a\x19\xC2V[\x95\x98\x94\x97P\x92\x95\x91\x94\x93a\x01 \x90\x92\x015\x92PPV[__`\xE0\x83\x85\x03\x12\x15a\x1D\xDEW__\xFD[a\x1D\xE8\x84\x84a\x1C\xB6V[\x91P`\xC0\x83\x015a\x1B0\x81a\x19\xC2V[________`\xE0\x89\x8B\x03\x12\x15a\x1E\x0FW__\xFD[\x885g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1E%W__\xFD[a\x1E1\x8B\x82\x8C\x01a\x1A5V[\x90\x99P\x97PP` \x89\x015\x95P`@\x89\x015a\x1EL\x81a\x19\xC2V[\x94P``\x89\x015\x93P`\x80\x89\x015\x92P`\xA0\x89\x015a\x1A\xF5\x81a\x19\xC2V[___``\x84\x86\x03\x12\x15a\x1E|W__\xFD[\x835\x92P` \x84\x015\x91P`@\x84\x015a\x1A\x13\x81a\x19\xC2V[___``\x84\x86\x03\x12\x15a\x1E\xA7W__\xFD[\x835a\x1E\xB2\x81a\x19\xC2V[\x92P` \x84\x015\x91P`@\x84\x015a\x1A\x13\x81a\x19\xC2V[_` \x82\x84\x03\x12\x15a\x1E\xD9W__\xFD[\x815a\x0C\xB8\x81a\x19\xC2V[\x81\x83R\x81\x81` \x85\x017P_\x82\x82\x01` \x90\x81\x01\x91\x90\x91R`\x1F\x90\x91\x01`\x1F\x19\x16\x90\x91\x01\x01\x90V[` \x81R_a\x15i` \x83\x01\x84\x86a\x1E\xE4V[\x80Qa\x1C\xB1\x81a\x19\xC2V[_`\xA0\x82\x84\x03\x12\x80\x15a\x1F;W__\xFD[Pa\x1FDa\x1C0V[\x82Qa\x1FO\x81a\x19\xC2V[\x81R` \x83\x81\x01Q\x90\x82\x01R`@\x83\x01Qa\x1Fi\x81a\x19\xC2V[`@\x82\x01R``\x83\x01Qa\x1F|\x81a\x19\xC2V[``\x82\x01R`\x80\x92\x83\x01Q\x92\x81\x01\x92\x90\x92RP\x91\x90PV[_\x82`\x1F\x83\x01\x12a\x1F\xA3W__\xFD[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x1F\xBDWa\x1F\xBDa\x1C\x1CV[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x1F\xECWa\x1F\xECa\x1C\x1CV[`@R\x81\x81R\x83\x82\x01` \x01\x85\x10\x15a \x03W__\xFD[\x81` \x85\x01` \x83\x01^_\x91\x81\x01` \x01\x91\x90\x91R\x93\x92PPPV[\x80Q\x80\x15\x15\x81\x14a\x1C\xB1W__\xFD[_` \x82\x84\x03\x12\x15a >W__\xFD[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a TW__\xFD[\x82\x01a\x01@\x81\x85\x03\x12\x15a fW__\xFD[a na\x1C|V[\x81Q\x81R` \x82\x01Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a \x8BW__\xFD[a \x97\x86\x82\x85\x01a\x1F\x94V[` \x83\x01RP`@\x82\x01Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a \xB6W__\xFD[a \xC2\x86\x82\x85\x01a\x1F\x94V[`@\x83\x01RPa \xD4``\x83\x01a\x1F\x1FV[``\x82\x01Ra \xE5`\x80\x83\x01a\x1F\x1FV[`\x80\x82\x01Ra \xF6`\xA0\x83\x01a\x1F\x1FV[`\xA0\x82\x01R`\xC0\x82\x81\x01Q\x90\x82\x01R`\xE0\x80\x83\x01Q\x90\x82\x01Ra!\x1Ca\x01\0\x83\x01a \x1FV[a\x01\0\x82\x01Ra!/a\x01 \x83\x01a \x1FV[a\x01 \x82\x01R\x94\x93PPPPV[\x93\x84R`\x01`\x01`\xA0\x1B\x03\x92\x83\x16` \x85\x01R`@\x84\x01\x91\x90\x91R\x16``\x82\x01R`\x80\x01\x90V[`\x01`\x01`\xA0\x1B\x03\x87\x16\x81R`\xA0` \x82\x01\x81\x90R_\x90a!\x88\x90\x83\x01\x87\x89a\x1E\xE4V[`@\x83\x01\x95\x90\x95RP`\x01`\x01`\xA0\x1B\x03\x92\x90\x92\x16``\x83\x01R`\x80\x90\x91\x01R\x93\x92PPPV[_a\x01\0\x82\x01\x90P\x84Q\x82R` \x85\x01Q` \x83\x01R`@\x85\x01Q`@\x83\x01R``\x85\x01Q``\x83\x01_[`\x02\x81\x10\x15a!\xF9W\x82Q\x82R` \x92\x83\x01\x92\x90\x91\x01\x90`\x01\x01a!\xDAV[PPP`\x80\x85\x01Qa\xFF\xFF\x16`\xA0\x83\x01R`\x01`\x01`\xA0\x1B\x03\x84\x16`\xC0\x83\x01R`\x01`\x01`\xA0\x1B\x03\x83\x16`\xE0\x83\x01Ra\x15iV\xFE\x97fpp\xC5N\xF1\x82\xB0\xF5\x85\x8B\x03K\xEA\xC1\xB6\xF3\x08\x9A\xA2\xD3\x18\x8B\xB1\xE8\x92\x9FO\xA9\xB9)\xA2dipfsX\"\x12 \x14\x92\xFE\xF9\x15\xBA\xF9\xE1\x9F\xE3\xE5>6`8\x90\x94<\xF5\x88\xD0\xD7y\xF0\xC7\x1D\xB4\x183\x07WjdsolcC\0\x08\x1C\x003",
    );
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `AccessControlBadConfirmation()` and selector `0x6697b232`.
```solidity
error AccessControlBadConfirmation();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct AccessControlBadConfirmation;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<AccessControlBadConfirmation>
        for UnderlyingRustTuple<'_> {
            fn from(value: AccessControlBadConfirmation) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for AccessControlBadConfirmation {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for AccessControlBadConfirmation {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "AccessControlBadConfirmation()";
            const SELECTOR: [u8; 4] = [102u8, 151u8, 178u8, 50u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `AccessControlUnauthorizedAccount(address,bytes32)` and selector `0xe2517d3f`.
```solidity
error AccessControlUnauthorizedAccount(address account, bytes32 neededRole);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct AccessControlUnauthorizedAccount {
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub neededRole: alloy::sol_types::private::FixedBytes<32>,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = (
            alloy::sol_types::sol_data::Address,
            alloy::sol_types::sol_data::FixedBytes<32>,
        );
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = (
            alloy::sol_types::private::Address,
            alloy::sol_types::private::FixedBytes<32>,
        );
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<AccessControlUnauthorizedAccount>
        for UnderlyingRustTuple<'_> {
            fn from(value: AccessControlUnauthorizedAccount) -> Self {
                (value.account, value.neededRole)
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for AccessControlUnauthorizedAccount {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self {
                    account: tuple.0,
                    neededRole: tuple.1,
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for AccessControlUnauthorizedAccount {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "AccessControlUnauthorizedAccount(address,bytes32)";
            const SELECTOR: [u8; 4] = [226u8, 81u8, 125u8, 63u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.account,
                    ),
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.neededRole),
                )
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `AmountMismatch()` and selector `0x55e97b0d`.
```solidity
error AmountMismatch();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct AmountMismatch;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<AmountMismatch> for UnderlyingRustTuple<'_> {
            fn from(value: AmountMismatch) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for AmountMismatch {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for AmountMismatch {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "AmountMismatch()";
            const SELECTOR: [u8; 4] = [85u8, 233u8, 123u8, 13u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `DeploymentFailed()` and selector `0x30116425`.
```solidity
error DeploymentFailed();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct DeploymentFailed;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<DeploymentFailed> for UnderlyingRustTuple<'_> {
            fn from(value: DeploymentFailed) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for DeploymentFailed {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for DeploymentFailed {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "DeploymentFailed()";
            const SELECTOR: [u8; 4] = [48u8, 17u8, 100u8, 37u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `FailedDeployment()` and selector `0xb06ebf3d`.
```solidity
error FailedDeployment();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct FailedDeployment;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<FailedDeployment> for UnderlyingRustTuple<'_> {
            fn from(value: FailedDeployment) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for FailedDeployment {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for FailedDeployment {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "FailedDeployment()";
            const SELECTOR: [u8; 4] = [176u8, 110u8, 191u8, 61u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `InsufficientBalance(uint256,uint256)` and selector `0xcf479181`.
```solidity
error InsufficientBalance(uint256 balance, uint256 needed);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InsufficientBalance {
        #[allow(missing_docs)]
        pub balance: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub needed: alloy::sol_types::private::primitives::aliases::U256,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = (
            alloy::sol_types::sol_data::Uint<256>,
            alloy::sol_types::sol_data::Uint<256>,
        );
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = (
            alloy::sol_types::private::primitives::aliases::U256,
            alloy::sol_types::private::primitives::aliases::U256,
        );
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<InsufficientBalance> for UnderlyingRustTuple<'_> {
            fn from(value: InsufficientBalance) -> Self {
                (value.balance, value.needed)
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for InsufficientBalance {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self {
                    balance: tuple.0,
                    needed: tuple.1,
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InsufficientBalance {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InsufficientBalance(uint256,uint256)";
            const SELECTOR: [u8; 4] = [207u8, 71u8, 145u8, 129u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.balance),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.needed),
                )
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `InvalidLiFiDestinationChain()` and selector `0x97c91b69`.
```solidity
error InvalidLiFiDestinationChain();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidLiFiDestinationChain;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<InvalidLiFiDestinationChain>
        for UnderlyingRustTuple<'_> {
            fn from(value: InvalidLiFiDestinationChain) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for InvalidLiFiDestinationChain {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidLiFiDestinationChain {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidLiFiDestinationChain()";
            const SELECTOR: [u8; 4] = [151u8, 201u8, 27u8, 105u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `InvalidLiFiReceiver()` and selector `0x523660f7`.
```solidity
error InvalidLiFiReceiver();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidLiFiReceiver;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<InvalidLiFiReceiver> for UnderlyingRustTuple<'_> {
            fn from(value: InvalidLiFiReceiver) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for InvalidLiFiReceiver {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidLiFiReceiver {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidLiFiReceiver()";
            const SELECTOR: [u8; 4] = [82u8, 54u8, 96u8, 247u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `OwnableInvalidOwner(address)` and selector `0x1e4fbdf7`.
```solidity
error OwnableInvalidOwner(address owner);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct OwnableInvalidOwner {
        #[allow(missing_docs)]
        pub owner: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<OwnableInvalidOwner> for UnderlyingRustTuple<'_> {
            fn from(value: OwnableInvalidOwner) -> Self {
                (value.owner,)
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for OwnableInvalidOwner {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self { owner: tuple.0 }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for OwnableInvalidOwner {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "OwnableInvalidOwner(address)";
            const SELECTOR: [u8; 4] = [30u8, 79u8, 189u8, 247u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.owner,
                    ),
                )
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `OwnableUnauthorizedAccount(address)` and selector `0x118cdaa7`.
```solidity
error OwnableUnauthorizedAccount(address account);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct OwnableUnauthorizedAccount {
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<OwnableUnauthorizedAccount>
        for UnderlyingRustTuple<'_> {
            fn from(value: OwnableUnauthorizedAccount) -> Self {
                (value.account,)
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for OwnableUnauthorizedAccount {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self { account: tuple.0 }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for OwnableUnauthorizedAccount {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "OwnableUnauthorizedAccount(address)";
            const SELECTOR: [u8; 4] = [17u8, 140u8, 218u8, 167u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.account,
                    ),
                )
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `UnsupportedBridging()` and selector `0x437f3ac3`.
```solidity
error UnsupportedBridging();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct UnsupportedBridging;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnsupportedBridging> for UnderlyingRustTuple<'_> {
            fn from(value: UnsupportedBridging) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for UnsupportedBridging {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for UnsupportedBridging {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "UnsupportedBridging()";
            const SELECTOR: [u8; 4] = [67u8, 127u8, 58u8, 195u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `UnsupportedShielding()` and selector `0x89da714f`.
```solidity
error UnsupportedShielding();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct UnsupportedShielding;
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[doc(hidden)]
        #[allow(dead_code)]
        type UnderlyingSolTuple<'a> = ();
        #[doc(hidden)]
        type UnderlyingRustTuple<'a> = ();
        #[cfg(test)]
        #[allow(dead_code, unreachable_patterns)]
        fn _type_assertion(
            _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
        ) {
            match _t {
                alloy_sol_types::private::AssertTypeEq::<
                    <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                >(_) => {}
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnsupportedShielding> for UnderlyingRustTuple<'_> {
            fn from(value: UnsupportedShielding) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for UnsupportedShielding {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for UnsupportedShielding {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "UnsupportedShielding()";
            const SELECTOR: [u8; 4] = [137u8, 218u8, 113u8, 79u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn abi_decode_raw_validate(data: &[u8]) -> alloy_sol_types::Result<Self> {
                <Self::Parameters<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Self::new)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `ConfigUpdated(address,address,address)` and selector `0xff8a97fda7728495ee3a5c551af3495e4ba23cdda2ac138491eff294902122ee`.
```solidity
event ConfigUpdated(address curvyVaultProxyAddress, address curvyAggregatorAlphaProxyAddress, address lifiDiamondAddress);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct ConfigUpdated {
        #[allow(missing_docs)]
        pub curvyVaultProxyAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub curvyAggregatorAlphaProxyAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub lifiDiamondAddress: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for ConfigUpdated {
            type DataTuple<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (alloy_sol_types::sol_data::FixedBytes<32>,);
            const SIGNATURE: &'static str = "ConfigUpdated(address,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                255u8, 138u8, 151u8, 253u8, 167u8, 114u8, 132u8, 149u8, 238u8, 58u8,
                92u8, 85u8, 26u8, 243u8, 73u8, 94u8, 75u8, 162u8, 60u8, 221u8, 162u8,
                172u8, 19u8, 132u8, 145u8, 239u8, 242u8, 148u8, 144u8, 33u8, 34u8, 238u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    curvyVaultProxyAddress: data.0,
                    curvyAggregatorAlphaProxyAddress: data.1,
                    lifiDiamondAddress: data.2,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.curvyVaultProxyAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.curvyAggregatorAlphaProxyAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.lifiDiamondAddress,
                    ),
                )
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (Self::SIGNATURE_HASH.into(),)
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for ConfigUpdated {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&ConfigUpdated> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &ConfigUpdated) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `EntryBridgePortalDeployed(address,uint256,address,address)` and selector `0x0d580c10960bd9b30908da4edf8e5f5ba6b1fbaf2c32d28be175a92d4b9896f2`.
```solidity
event EntryBridgePortalDeployed(address indexed portalAddress, uint256 indexed ownerHash, address indexed recovery, address currency);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct EntryBridgePortalDeployed {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub ownerHash: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for EntryBridgePortalDeployed {
            type DataTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "EntryBridgePortalDeployed(address,uint256,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                13u8, 88u8, 12u8, 16u8, 150u8, 11u8, 217u8, 179u8, 9u8, 8u8, 218u8, 78u8,
                223u8, 142u8, 95u8, 91u8, 166u8, 177u8, 251u8, 175u8, 44u8, 50u8, 210u8,
                139u8, 225u8, 117u8, 169u8, 45u8, 75u8, 152u8, 150u8, 242u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    portalAddress: topics.1,
                    ownerHash: topics.2,
                    recovery: topics.3,
                    currency: data.0,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                )
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.portalAddress.clone(),
                    self.ownerHash.clone(),
                    self.recovery.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.portalAddress,
                );
                out[2usize] = <alloy::sol_types::sol_data::Uint<
                    256,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.ownerHash);
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.recovery,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for EntryBridgePortalDeployed {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&EntryBridgePortalDeployed> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(
                this: &EntryBridgePortalDeployed,
            ) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `ExitBridgePortalDeployed(address,address,uint256,address,address)` and selector `0xd32efcefa11997ebea13e4c365a2a92eefc7b965a74ba151dadb02a3cd500637`.
```solidity
event ExitBridgePortalDeployed(address indexed portalAddress, address indexed exitAddress, uint256 exitChainId, address indexed recovery, address currency);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct ExitBridgePortalDeployed {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for ExitBridgePortalDeployed {
            type DataTuple<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "ExitBridgePortalDeployed(address,address,uint256,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                211u8, 46u8, 252u8, 239u8, 161u8, 25u8, 151u8, 235u8, 234u8, 19u8, 228u8,
                195u8, 101u8, 162u8, 169u8, 46u8, 239u8, 199u8, 185u8, 101u8, 167u8,
                75u8, 161u8, 81u8, 218u8, 219u8, 2u8, 163u8, 205u8, 80u8, 6u8, 55u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    portalAddress: topics.1,
                    exitAddress: topics.2,
                    exitChainId: data.0,
                    recovery: topics.3,
                    currency: data.1,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                (
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                )
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.portalAddress.clone(),
                    self.exitAddress.clone(),
                    self.recovery.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.portalAddress,
                );
                out[2usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.exitAddress,
                );
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.recovery,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for ExitBridgePortalDeployed {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&ExitBridgePortalDeployed> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(
                this: &ExitBridgePortalDeployed,
            ) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `OwnershipTransferred(address,address)` and selector `0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0`.
```solidity
event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct OwnershipTransferred {
        #[allow(missing_docs)]
        pub previousOwner: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub newOwner: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for OwnershipTransferred {
            type DataTuple<'a> = ();
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "OwnershipTransferred(address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                139u8, 224u8, 7u8, 156u8, 83u8, 22u8, 89u8, 20u8, 19u8, 68u8, 205u8,
                31u8, 208u8, 164u8, 242u8, 132u8, 25u8, 73u8, 127u8, 151u8, 34u8, 163u8,
                218u8, 175u8, 227u8, 180u8, 24u8, 111u8, 107u8, 100u8, 87u8, 224u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    previousOwner: topics.1,
                    newOwner: topics.2,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                ()
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.previousOwner.clone(),
                    self.newOwner.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.previousOwner,
                );
                out[2usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.newOwner,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for OwnershipTransferred {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&OwnershipTransferred> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &OwnershipTransferred) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `RecoveryPortalDeployed(address,address,address)` and selector `0x248ad0b6173546f5b68ca1ea8493871fb220561b577d7d5fdf7bf404c1c1f879`.
```solidity
event RecoveryPortalDeployed(address indexed portalAddress, address indexed tokenAddress, address indexed to);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct RecoveryPortalDeployed {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub tokenAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub to: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for RecoveryPortalDeployed {
            type DataTuple<'a> = ();
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "RecoveryPortalDeployed(address,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                36u8, 138u8, 208u8, 182u8, 23u8, 53u8, 70u8, 245u8, 182u8, 140u8, 161u8,
                234u8, 132u8, 147u8, 135u8, 31u8, 178u8, 32u8, 86u8, 27u8, 87u8, 125u8,
                125u8, 95u8, 223u8, 123u8, 244u8, 4u8, 193u8, 193u8, 248u8, 121u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    portalAddress: topics.1,
                    tokenAddress: topics.2,
                    to: topics.3,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                ()
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.portalAddress.clone(),
                    self.tokenAddress.clone(),
                    self.to.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.portalAddress,
                );
                out[2usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.tokenAddress,
                );
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.to,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for RecoveryPortalDeployed {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&RecoveryPortalDeployed> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &RecoveryPortalDeployed) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `RoleAdminChanged(bytes32,bytes32,bytes32)` and selector `0xbd79b86ffe0ab8e8776151514217cd7cacd52c909f66475c3af44e129f0b00ff`.
```solidity
event RoleAdminChanged(bytes32 indexed role, bytes32 indexed previousAdminRole, bytes32 indexed newAdminRole);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct RoleAdminChanged {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub previousAdminRole: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub newAdminRole: alloy::sol_types::private::FixedBytes<32>,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for RoleAdminChanged {
            type DataTuple<'a> = ();
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::FixedBytes<32>,
            );
            const SIGNATURE: &'static str = "RoleAdminChanged(bytes32,bytes32,bytes32)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                189u8, 121u8, 184u8, 111u8, 254u8, 10u8, 184u8, 232u8, 119u8, 97u8, 81u8,
                81u8, 66u8, 23u8, 205u8, 124u8, 172u8, 213u8, 44u8, 144u8, 159u8, 102u8,
                71u8, 92u8, 58u8, 244u8, 78u8, 18u8, 159u8, 11u8, 0u8, 255u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    role: topics.1,
                    previousAdminRole: topics.2,
                    newAdminRole: topics.3,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                ()
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.role.clone(),
                    self.previousAdminRole.clone(),
                    self.newAdminRole.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.role);
                out[2usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.previousAdminRole);
                out[3usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.newAdminRole);
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for RoleAdminChanged {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&RoleAdminChanged> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &RoleAdminChanged) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `RoleGranted(bytes32,address,address)` and selector `0x2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d`.
```solidity
event RoleGranted(bytes32 indexed role, address indexed account, address indexed sender);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct RoleGranted {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub sender: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for RoleGranted {
            type DataTuple<'a> = ();
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "RoleGranted(bytes32,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                47u8, 135u8, 136u8, 17u8, 126u8, 126u8, 255u8, 29u8, 130u8, 233u8, 38u8,
                236u8, 121u8, 73u8, 1u8, 209u8, 124u8, 120u8, 2u8, 74u8, 80u8, 39u8, 9u8,
                64u8, 48u8, 69u8, 64u8, 167u8, 51u8, 101u8, 111u8, 13u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    role: topics.1,
                    account: topics.2,
                    sender: topics.3,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                ()
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.role.clone(),
                    self.account.clone(),
                    self.sender.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.role);
                out[2usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.account,
                );
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.sender,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for RoleGranted {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&RoleGranted> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &RoleGranted) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `RoleRevoked(bytes32,address,address)` and selector `0xf6391f5c32d9c69d2a47ea670b442974b53935d1edc7fd64eb21e047a839171b`.
```solidity
event RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct RoleRevoked {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub sender: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for RoleRevoked {
            type DataTuple<'a> = ();
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "RoleRevoked(bytes32,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                246u8, 57u8, 31u8, 92u8, 50u8, 217u8, 198u8, 157u8, 42u8, 71u8, 234u8,
                103u8, 11u8, 68u8, 41u8, 116u8, 181u8, 57u8, 53u8, 209u8, 237u8, 199u8,
                253u8, 100u8, 235u8, 33u8, 224u8, 71u8, 168u8, 57u8, 23u8, 27u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    role: topics.1,
                    account: topics.2,
                    sender: topics.3,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                ()
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.role.clone(),
                    self.account.clone(),
                    self.sender.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.role);
                out[2usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.account,
                );
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.sender,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for RoleRevoked {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&RoleRevoked> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &RoleRevoked) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `ShieldPortalDeployed(address,uint256,address)` and selector `0x7153fc1ba1d8a6784e0a3aa1113e319ebf6702ccbdcc83a126a5d23b2671b074`.
```solidity
event ShieldPortalDeployed(address indexed portalAddress, uint256 indexed ownerHash, address indexed recovery);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct ShieldPortalDeployed {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub ownerHash: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for ShieldPortalDeployed {
            type DataTuple<'a> = ();
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "ShieldPortalDeployed(address,uint256,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                113u8, 83u8, 252u8, 27u8, 161u8, 216u8, 166u8, 120u8, 78u8, 10u8, 58u8,
                161u8, 17u8, 62u8, 49u8, 158u8, 191u8, 103u8, 2u8, 204u8, 189u8, 204u8,
                131u8, 161u8, 38u8, 165u8, 210u8, 59u8, 38u8, 113u8, 176u8, 116u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    portalAddress: topics.1,
                    ownerHash: topics.2,
                    recovery: topics.3,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                ()
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.portalAddress.clone(),
                    self.ownerHash.clone(),
                    self.recovery.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.portalAddress,
                );
                out[2usize] = <alloy::sol_types::sol_data::Uint<
                    256,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.ownerHash);
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.recovery,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for ShieldPortalDeployed {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&ShieldPortalDeployed> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(this: &ShieldPortalDeployed) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `SolanaExitBridgePortalDeployed(address,bytes32,uint256,address,address)` and selector `0x2159a9beac3a6bdf488be58a40cb4df3c7649082933a290f1d86a43ebba2d213`.
```solidity
event SolanaExitBridgePortalDeployed(address indexed portalAddress, bytes32 indexed exitAddress, uint256 exitChainId, address indexed recovery, address currency);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct SolanaExitBridgePortalDeployed {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for SolanaExitBridgePortalDeployed {
            type DataTuple<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "SolanaExitBridgePortalDeployed(address,bytes32,uint256,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                33u8, 89u8, 169u8, 190u8, 172u8, 58u8, 107u8, 223u8, 72u8, 139u8, 229u8,
                138u8, 64u8, 203u8, 77u8, 243u8, 199u8, 100u8, 144u8, 130u8, 147u8, 58u8,
                41u8, 15u8, 29u8, 134u8, 164u8, 62u8, 187u8, 162u8, 210u8, 19u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    portalAddress: topics.1,
                    exitAddress: topics.2,
                    exitChainId: data.0,
                    recovery: topics.3,
                    currency: data.1,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                (
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                )
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.portalAddress.clone(),
                    self.exitAddress.clone(),
                    self.recovery.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.portalAddress,
                );
                out[2usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.exitAddress);
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.recovery,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for SolanaExitBridgePortalDeployed {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&SolanaExitBridgePortalDeployed>
        for alloy_sol_types::private::LogData {
            #[inline]
            fn from(
                this: &SolanaExitBridgePortalDeployed,
            ) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Event with signature `SolanaRecoveryPortalDeployed(address,bytes32,address,address)` and selector `0x2113859f5b6983a70726932ab3518e5bf454f731fdb16d96d8ac870a5b232241`.
```solidity
event SolanaRecoveryPortalDeployed(address indexed portalAddress, bytes32 indexed exitAddress, address indexed tokenAddress, address to);
```*/
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    #[derive(Clone)]
    pub struct SolanaRecoveryPortalDeployed {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub tokenAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub to: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        #[automatically_derived]
        impl alloy_sol_types::SolEvent for SolanaRecoveryPortalDeployed {
            type DataTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type DataToken<'a> = <Self::DataTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type TopicList = (
                alloy_sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            const SIGNATURE: &'static str = "SolanaRecoveryPortalDeployed(address,bytes32,address,address)";
            const SIGNATURE_HASH: alloy_sol_types::private::B256 = alloy_sol_types::private::B256::new([
                33u8, 19u8, 133u8, 159u8, 91u8, 105u8, 131u8, 167u8, 7u8, 38u8, 147u8,
                42u8, 179u8, 81u8, 142u8, 91u8, 244u8, 84u8, 247u8, 49u8, 253u8, 177u8,
                109u8, 150u8, 216u8, 172u8, 135u8, 10u8, 91u8, 35u8, 34u8, 65u8,
            ]);
            const ANONYMOUS: bool = false;
            #[allow(unused_variables)]
            #[inline]
            fn new(
                topics: <Self::TopicList as alloy_sol_types::SolType>::RustType,
                data: <Self::DataTuple<'_> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                Self {
                    portalAddress: topics.1,
                    exitAddress: topics.2,
                    tokenAddress: topics.3,
                    to: data.0,
                }
            }
            #[inline]
            fn check_signature(
                topics: &<Self::TopicList as alloy_sol_types::SolType>::RustType,
            ) -> alloy_sol_types::Result<()> {
                if topics.0 != Self::SIGNATURE_HASH {
                    return Err(
                        alloy_sol_types::Error::invalid_event_signature_hash(
                            Self::SIGNATURE,
                            topics.0,
                            Self::SIGNATURE_HASH,
                        ),
                    );
                }
                Ok(())
            }
            #[inline]
            fn tokenize_body(&self) -> Self::DataToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.to,
                    ),
                )
            }
            #[inline]
            fn topics(&self) -> <Self::TopicList as alloy_sol_types::SolType>::RustType {
                (
                    Self::SIGNATURE_HASH.into(),
                    self.portalAddress.clone(),
                    self.exitAddress.clone(),
                    self.tokenAddress.clone(),
                )
            }
            #[inline]
            fn encode_topics_raw(
                &self,
                out: &mut [alloy_sol_types::abi::token::WordToken],
            ) -> alloy_sol_types::Result<()> {
                if out.len() < <Self::TopicList as alloy_sol_types::TopicList>::COUNT {
                    return Err(alloy_sol_types::Error::Overrun);
                }
                out[0usize] = alloy_sol_types::abi::token::WordToken(
                    Self::SIGNATURE_HASH,
                );
                out[1usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.portalAddress,
                );
                out[2usize] = <alloy::sol_types::sol_data::FixedBytes<
                    32,
                > as alloy_sol_types::EventTopic>::encode_topic(&self.exitAddress);
                out[3usize] = <alloy::sol_types::sol_data::Address as alloy_sol_types::EventTopic>::encode_topic(
                    &self.tokenAddress,
                );
                Ok(())
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::private::IntoLogData for SolanaRecoveryPortalDeployed {
            fn to_log_data(&self) -> alloy_sol_types::private::LogData {
                From::from(self)
            }
            fn into_log_data(self) -> alloy_sol_types::private::LogData {
                From::from(&self)
            }
        }
        #[automatically_derived]
        impl From<&SolanaRecoveryPortalDeployed> for alloy_sol_types::private::LogData {
            #[inline]
            fn from(
                this: &SolanaRecoveryPortalDeployed,
            ) -> alloy_sol_types::private::LogData {
                alloy_sol_types::SolEvent::encode_log_data(this)
            }
        }
    };
    /**Constructor`.
```solidity
constructor(address initialOwner);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct constructorCall {
        #[allow(missing_docs)]
        pub initialOwner: alloy::sol_types::private::Address,
    }
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<constructorCall> for UnderlyingRustTuple<'_> {
                fn from(value: constructorCall) -> Self {
                    (value.initialOwner,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for constructorCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { initialOwner: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolConstructor for constructorCall {
            type Parameters<'a> = (alloy::sol_types::sol_data::Address,);
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.initialOwner,
                    ),
                )
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `AUTHORITY_ROLE()` and selector `0x4a3fba0e`.
```solidity
function AUTHORITY_ROLE() external view returns (bytes32);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct AUTHORITY_ROLECall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`AUTHORITY_ROLE()`](AUTHORITY_ROLECall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct AUTHORITY_ROLEReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::FixedBytes<32>,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<AUTHORITY_ROLECall> for UnderlyingRustTuple<'_> {
                fn from(value: AUTHORITY_ROLECall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for AUTHORITY_ROLECall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::FixedBytes<32>,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<AUTHORITY_ROLEReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: AUTHORITY_ROLEReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for AUTHORITY_ROLEReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for AUTHORITY_ROLECall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::FixedBytes<32>;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "AUTHORITY_ROLE()";
            const SELECTOR: [u8; 4] = [74u8, 63u8, 186u8, 14u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(ret),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: AUTHORITY_ROLEReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: AUTHORITY_ROLEReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `DEFAULT_ADMIN_ROLE()` and selector `0xa217fddf`.
```solidity
function DEFAULT_ADMIN_ROLE() external view returns (bytes32);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct DEFAULT_ADMIN_ROLECall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`DEFAULT_ADMIN_ROLE()`](DEFAULT_ADMIN_ROLECall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct DEFAULT_ADMIN_ROLEReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::FixedBytes<32>,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<DEFAULT_ADMIN_ROLECall>
            for UnderlyingRustTuple<'_> {
                fn from(value: DEFAULT_ADMIN_ROLECall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for DEFAULT_ADMIN_ROLECall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::FixedBytes<32>,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<DEFAULT_ADMIN_ROLEReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: DEFAULT_ADMIN_ROLEReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for DEFAULT_ADMIN_ROLEReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for DEFAULT_ADMIN_ROLECall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::FixedBytes<32>;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "DEFAULT_ADMIN_ROLE()";
            const SELECTOR: [u8; 4] = [162u8, 23u8, 253u8, 223u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(ret),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: DEFAULT_ADMIN_ROLEReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: DEFAULT_ADMIN_ROLEReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `OPERATOR_ROLE()` and selector `0xf5b541a6`.
```solidity
function OPERATOR_ROLE() external view returns (bytes32);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct OPERATOR_ROLECall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`OPERATOR_ROLE()`](OPERATOR_ROLECall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct OPERATOR_ROLEReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::FixedBytes<32>,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<OPERATOR_ROLECall> for UnderlyingRustTuple<'_> {
                fn from(value: OPERATOR_ROLECall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for OPERATOR_ROLECall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::FixedBytes<32>,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<OPERATOR_ROLEReturn> for UnderlyingRustTuple<'_> {
                fn from(value: OPERATOR_ROLEReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for OPERATOR_ROLEReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for OPERATOR_ROLECall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::FixedBytes<32>;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "OPERATOR_ROLE()";
            const SELECTOR: [u8; 4] = [245u8, 181u8, 65u8, 166u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(ret),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: OPERATOR_ROLEReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: OPERATOR_ROLEReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deployEntryBridgePortal(bytes,(uint256,uint256,uint256,uint256[2],uint16),address,address,uint256)` and selector `0x848c9f82`.
```solidity
function deployEntryBridgePortal(bytes memory bridgeData, CurvyTypes.Note memory note, address currency, address recovery, uint256 gasFee) external payable;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployEntryBridgePortalCall {
        #[allow(missing_docs)]
        pub bridgeData: alloy::sol_types::private::Bytes,
        #[allow(missing_docs)]
        pub note: <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub gasFee: alloy::sol_types::private::primitives::aliases::U256,
    }
    ///Container type for the return parameters of the [`deployEntryBridgePortal(bytes,(uint256,uint256,uint256,uint256[2],uint16),address,address,uint256)`](deployEntryBridgePortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployEntryBridgePortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Bytes,
                CurvyTypes::Note,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Bytes,
                <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::primitives::aliases::U256,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployEntryBridgePortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployEntryBridgePortalCall) -> Self {
                    (
                        value.bridgeData,
                        value.note,
                        value.currency,
                        value.recovery,
                        value.gasFee,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployEntryBridgePortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        bridgeData: tuple.0,
                        note: tuple.1,
                        currency: tuple.2,
                        recovery: tuple.3,
                        gasFee: tuple.4,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployEntryBridgePortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployEntryBridgePortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployEntryBridgePortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deployEntryBridgePortalReturn {
            fn _tokenize(
                &self,
            ) -> <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::ReturnToken<
                '_,
            > {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deployEntryBridgePortalCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Bytes,
                CurvyTypes::Note,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deployEntryBridgePortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deployEntryBridgePortal(bytes,(uint256,uint256,uint256,uint256[2],uint16),address,address,uint256)";
            const SELECTOR: [u8; 4] = [132u8, 140u8, 159u8, 130u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Bytes as alloy_sol_types::SolType>::tokenize(
                        &self.bridgeData,
                    ),
                    <CurvyTypes::Note as alloy_sol_types::SolType>::tokenize(&self.note),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.gasFee),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deployEntryBridgePortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deployExitBridgePortal(bytes,uint256,address,address,uint256,address,uint256)` and selector `0x2a33cf2e`.
```solidity
function deployExitBridgePortal(bytes memory bridgeData, uint256 amount, address currency, address exitAddress, uint256 exitChainId, address recovery, uint256 gasFee) external payable;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployExitBridgePortalCall {
        #[allow(missing_docs)]
        pub bridgeData: alloy::sol_types::private::Bytes,
        #[allow(missing_docs)]
        pub amount: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub gasFee: alloy::sol_types::private::primitives::aliases::U256,
    }
    ///Container type for the return parameters of the [`deployExitBridgePortal(bytes,uint256,address,address,uint256,address,uint256)`](deployExitBridgePortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployExitBridgePortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Bytes,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Bytes,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::primitives::aliases::U256,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployExitBridgePortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployExitBridgePortalCall) -> Self {
                    (
                        value.bridgeData,
                        value.amount,
                        value.currency,
                        value.exitAddress,
                        value.exitChainId,
                        value.recovery,
                        value.gasFee,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployExitBridgePortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        bridgeData: tuple.0,
                        amount: tuple.1,
                        currency: tuple.2,
                        exitAddress: tuple.3,
                        exitChainId: tuple.4,
                        recovery: tuple.5,
                        gasFee: tuple.6,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployExitBridgePortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployExitBridgePortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployExitBridgePortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deployExitBridgePortalReturn {
            fn _tokenize(
                &self,
            ) -> <deployExitBridgePortalCall as alloy_sol_types::SolCall>::ReturnToken<
                '_,
            > {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deployExitBridgePortalCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Bytes,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deployExitBridgePortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deployExitBridgePortal(bytes,uint256,address,address,uint256,address,uint256)";
            const SELECTOR: [u8; 4] = [42u8, 51u8, 207u8, 46u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Bytes as alloy_sol_types::SolType>::tokenize(
                        &self.bridgeData,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.amount),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.exitAddress,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.gasFee),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deployExitBridgePortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deployRecoveryEntryPortal(uint256,address,address,address)` and selector `0x53070b55`.
```solidity
function deployRecoveryEntryPortal(uint256 ownerHash, address recovery, address tokenAddress, address to) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployRecoveryEntryPortalCall {
        #[allow(missing_docs)]
        pub ownerHash: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub tokenAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub to: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`deployRecoveryEntryPortal(uint256,address,address,address)`](deployRecoveryEntryPortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployRecoveryEntryPortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployRecoveryEntryPortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployRecoveryEntryPortalCall) -> Self {
                    (value.ownerHash, value.recovery, value.tokenAddress, value.to)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployRecoveryEntryPortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        ownerHash: tuple.0,
                        recovery: tuple.1,
                        tokenAddress: tuple.2,
                        to: tuple.3,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployRecoveryEntryPortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployRecoveryEntryPortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployRecoveryEntryPortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deployRecoveryEntryPortalReturn {
            fn _tokenize(
                &self,
            ) -> <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::ReturnToken<
                '_,
            > {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deployRecoveryEntryPortalCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deployRecoveryEntryPortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deployRecoveryEntryPortal(uint256,address,address,address)";
            const SELECTOR: [u8; 4] = [83u8, 7u8, 11u8, 85u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.ownerHash),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.tokenAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.to,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deployRecoveryEntryPortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deployRecoveryExitPortal(address,uint256,address,address,address)` and selector `0x66e93b8c`.
```solidity
function deployRecoveryExitPortal(address exitAddress, uint256 exitChainId, address recovery, address tokenAddress, address to) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployRecoveryExitPortalCall {
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub tokenAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub to: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`deployRecoveryExitPortal(address,uint256,address,address,address)`](deployRecoveryExitPortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployRecoveryExitPortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Address,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployRecoveryExitPortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployRecoveryExitPortalCall) -> Self {
                    (
                        value.exitAddress,
                        value.exitChainId,
                        value.recovery,
                        value.tokenAddress,
                        value.to,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployRecoveryExitPortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        exitAddress: tuple.0,
                        exitChainId: tuple.1,
                        recovery: tuple.2,
                        tokenAddress: tuple.3,
                        to: tuple.4,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployRecoveryExitPortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployRecoveryExitPortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployRecoveryExitPortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deployRecoveryExitPortalReturn {
            fn _tokenize(
                &self,
            ) -> <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::ReturnToken<
                '_,
            > {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deployRecoveryExitPortalCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deployRecoveryExitPortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deployRecoveryExitPortal(address,uint256,address,address,address)";
            const SELECTOR: [u8; 4] = [102u8, 233u8, 59u8, 140u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.exitAddress,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.tokenAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.to,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deployRecoveryExitPortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deployShieldPortal((uint256,uint256,uint256,uint256[2],uint16),address)` and selector `0xb64b2a8a`.
```solidity
function deployShieldPortal(CurvyTypes.Note memory note, address recovery) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployShieldPortalCall {
        #[allow(missing_docs)]
        pub note: <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`deployShieldPortal((uint256,uint256,uint256,uint256[2],uint16),address)`](deployShieldPortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deployShieldPortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                CurvyTypes::Note,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployShieldPortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployShieldPortalCall) -> Self {
                    (value.note, value.recovery)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployShieldPortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        note: tuple.0,
                        recovery: tuple.1,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deployShieldPortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deployShieldPortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deployShieldPortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deployShieldPortalReturn {
            fn _tokenize(
                &self,
            ) -> <deployShieldPortalCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deployShieldPortalCall {
            type Parameters<'a> = (
                CurvyTypes::Note,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deployShieldPortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deployShieldPortal((uint256,uint256,uint256,uint256[2],uint16),address)";
            const SELECTOR: [u8; 4] = [182u8, 75u8, 42u8, 138u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <CurvyTypes::Note as alloy_sol_types::SolType>::tokenize(&self.note),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deployShieldPortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deploySolanaExitBridgePortal(bytes,uint256,address,bytes32,uint256,address,uint256)` and selector `0xbc3488c6`.
```solidity
function deploySolanaExitBridgePortal(bytes memory bridgeData, uint256 amount, address currency, bytes32 exitAddress, uint256 exitChainId, address recovery, uint256 gasFee) external payable;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deploySolanaExitBridgePortalCall {
        #[allow(missing_docs)]
        pub bridgeData: alloy::sol_types::private::Bytes,
        #[allow(missing_docs)]
        pub amount: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub gasFee: alloy::sol_types::private::primitives::aliases::U256,
    }
    ///Container type for the return parameters of the [`deploySolanaExitBridgePortal(bytes,uint256,address,bytes32,uint256,address,uint256)`](deploySolanaExitBridgePortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deploySolanaExitBridgePortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Bytes,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Bytes,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::primitives::aliases::U256,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deploySolanaExitBridgePortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deploySolanaExitBridgePortalCall) -> Self {
                    (
                        value.bridgeData,
                        value.amount,
                        value.currency,
                        value.exitAddress,
                        value.exitChainId,
                        value.recovery,
                        value.gasFee,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deploySolanaExitBridgePortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        bridgeData: tuple.0,
                        amount: tuple.1,
                        currency: tuple.2,
                        exitAddress: tuple.3,
                        exitChainId: tuple.4,
                        recovery: tuple.5,
                        gasFee: tuple.6,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deploySolanaExitBridgePortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deploySolanaExitBridgePortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deploySolanaExitBridgePortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deploySolanaExitBridgePortalReturn {
            fn _tokenize(
                &self,
            ) -> <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::ReturnToken<
                '_,
            > {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deploySolanaExitBridgePortalCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Bytes,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deploySolanaExitBridgePortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deploySolanaExitBridgePortal(bytes,uint256,address,bytes32,uint256,address,uint256)";
            const SELECTOR: [u8; 4] = [188u8, 52u8, 136u8, 198u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Bytes as alloy_sol_types::SolType>::tokenize(
                        &self.bridgeData,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.amount),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitAddress),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.gasFee),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deploySolanaExitBridgePortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `deploySolanaRecoveryExitPortal(bytes32,uint256,address,address,address)` and selector `0x2f3819e6`.
```solidity
function deploySolanaRecoveryExitPortal(bytes32 exitAddress, uint256 exitChainId, address recovery, address tokenAddress, address to) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deploySolanaRecoveryExitPortalCall {
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub tokenAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub to: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`deploySolanaRecoveryExitPortal(bytes32,uint256,address,address,address)`](deploySolanaRecoveryExitPortalCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct deploySolanaRecoveryExitPortalReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deploySolanaRecoveryExitPortalCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: deploySolanaRecoveryExitPortalCall) -> Self {
                    (
                        value.exitAddress,
                        value.exitChainId,
                        value.recovery,
                        value.tokenAddress,
                        value.to,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deploySolanaRecoveryExitPortalCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        exitAddress: tuple.0,
                        exitChainId: tuple.1,
                        recovery: tuple.2,
                        tokenAddress: tuple.3,
                        to: tuple.4,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<deploySolanaRecoveryExitPortalReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: deploySolanaRecoveryExitPortalReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for deploySolanaRecoveryExitPortalReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl deploySolanaRecoveryExitPortalReturn {
            fn _tokenize(
                &self,
            ) -> <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::ReturnToken<
                '_,
            > {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for deploySolanaRecoveryExitPortalCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = deploySolanaRecoveryExitPortalReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "deploySolanaRecoveryExitPortal(bytes32,uint256,address,address,address)";
            const SELECTOR: [u8; 4] = [47u8, 56u8, 25u8, 230u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitAddress),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.tokenAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.to,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                deploySolanaRecoveryExitPortalReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `getEntryPortalAddress(uint256,address)` and selector `0x5e8b95d2`.
```solidity
function getEntryPortalAddress(uint256 ownerHash, address recovery) external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getEntryPortalAddressCall {
        #[allow(missing_docs)]
        pub ownerHash: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`getEntryPortalAddress(uint256,address)`](getEntryPortalAddressCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getEntryPortalAddressReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getEntryPortalAddressCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: getEntryPortalAddressCall) -> Self {
                    (value.ownerHash, value.recovery)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for getEntryPortalAddressCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        ownerHash: tuple.0,
                        recovery: tuple.1,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getEntryPortalAddressReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: getEntryPortalAddressReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for getEntryPortalAddressReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for getEntryPortalAddressCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "getEntryPortalAddress(uint256,address)";
            const SELECTOR: [u8; 4] = [94u8, 139u8, 149u8, 210u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.ownerHash),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: getEntryPortalAddressReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: getEntryPortalAddressReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `getExitPortalAddress(address,uint256,address)` and selector `0xe16ca895`.
```solidity
function getExitPortalAddress(address exitAddress, uint256 exitChainId, address recovery) external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getExitPortalAddressCall {
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`getExitPortalAddress(address,uint256,address)`](getExitPortalAddressCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getExitPortalAddressReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Address,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getExitPortalAddressCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: getExitPortalAddressCall) -> Self {
                    (value.exitAddress, value.exitChainId, value.recovery)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for getExitPortalAddressCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        exitAddress: tuple.0,
                        exitChainId: tuple.1,
                        recovery: tuple.2,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getExitPortalAddressReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: getExitPortalAddressReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for getExitPortalAddressReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for getExitPortalAddressCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "getExitPortalAddress(address,uint256,address)";
            const SELECTOR: [u8; 4] = [225u8, 108u8, 168u8, 149u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.exitAddress,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: getExitPortalAddressReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: getExitPortalAddressReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `getRoleAdmin(bytes32)` and selector `0x248a9ca3`.
```solidity
function getRoleAdmin(bytes32 role) external view returns (bytes32);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getRoleAdminCall {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`getRoleAdmin(bytes32)`](getRoleAdminCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getRoleAdminReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::FixedBytes<32>,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::FixedBytes<32>,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getRoleAdminCall> for UnderlyingRustTuple<'_> {
                fn from(value: getRoleAdminCall) -> Self {
                    (value.role,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for getRoleAdminCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { role: tuple.0 }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::FixedBytes<32>,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getRoleAdminReturn> for UnderlyingRustTuple<'_> {
                fn from(value: getRoleAdminReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for getRoleAdminReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for getRoleAdminCall {
            type Parameters<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::FixedBytes<32>;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<32>,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "getRoleAdmin(bytes32)";
            const SELECTOR: [u8; 4] = [36u8, 138u8, 156u8, 163u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.role),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(ret),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: getRoleAdminReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: getRoleAdminReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `getSolanaExitPortalAddress(bytes32,uint256,address)` and selector `0xd80513b5`.
```solidity
function getSolanaExitPortalAddress(bytes32 exitAddress, uint256 exitChainId, address recovery) external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getSolanaExitPortalAddressCall {
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub recovery: alloy::sol_types::private::Address,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`getSolanaExitPortalAddress(bytes32,uint256,address)`](getSolanaExitPortalAddressCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct getSolanaExitPortalAddressReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::primitives::aliases::U256,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getSolanaExitPortalAddressCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: getSolanaExitPortalAddressCall) -> Self {
                    (value.exitAddress, value.exitChainId, value.recovery)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for getSolanaExitPortalAddressCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        exitAddress: tuple.0,
                        exitChainId: tuple.1,
                        recovery: tuple.2,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<getSolanaExitPortalAddressReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: getSolanaExitPortalAddressReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for getSolanaExitPortalAddressReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for getSolanaExitPortalAddressCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "getSolanaExitPortalAddress(bytes32,uint256,address)";
            const SELECTOR: [u8; 4] = [216u8, 5u8, 19u8, 181u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitAddress),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.recovery,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: getSolanaExitPortalAddressReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: getSolanaExitPortalAddressReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `grantRole(bytes32,address)` and selector `0x2f2ff15d`.
```solidity
function grantRole(bytes32 role, address account) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct grantRoleCall {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`grantRole(bytes32,address)`](grantRoleCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct grantRoleReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<grantRoleCall> for UnderlyingRustTuple<'_> {
                fn from(value: grantRoleCall) -> Self {
                    (value.role, value.account)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for grantRoleCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        role: tuple.0,
                        account: tuple.1,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<grantRoleReturn> for UnderlyingRustTuple<'_> {
                fn from(value: grantRoleReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for grantRoleReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl grantRoleReturn {
            fn _tokenize(
                &self,
            ) -> <grantRoleCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for grantRoleCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = grantRoleReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "grantRole(bytes32,address)";
            const SELECTOR: [u8; 4] = [47u8, 47u8, 241u8, 93u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.role),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.account,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                grantRoleReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `hasRole(bytes32,address)` and selector `0x91d14854`.
```solidity
function hasRole(bytes32 role, address account) external view returns (bool);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct hasRoleCall {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`hasRole(bytes32,address)`](hasRoleCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct hasRoleReturn {
        #[allow(missing_docs)]
        pub _0: bool,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<hasRoleCall> for UnderlyingRustTuple<'_> {
                fn from(value: hasRoleCall) -> Self {
                    (value.role, value.account)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for hasRoleCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        role: tuple.0,
                        account: tuple.1,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (bool,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<hasRoleReturn> for UnderlyingRustTuple<'_> {
                fn from(value: hasRoleReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for hasRoleReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for hasRoleCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = bool;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "hasRole(bytes32,address)";
            const SELECTOR: [u8; 4] = [145u8, 209u8, 72u8, 84u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.role),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.account,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Bool as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: hasRoleReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: hasRoleReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `owner()` and selector `0x8da5cb5b`.
```solidity
function owner() external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct ownerCall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`owner()`](ownerCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct ownerReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<ownerCall> for UnderlyingRustTuple<'_> {
                fn from(value: ownerCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for ownerCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<ownerReturn> for UnderlyingRustTuple<'_> {
                fn from(value: ownerReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for ownerReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for ownerCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "owner()";
            const SELECTOR: [u8; 4] = [141u8, 165u8, 203u8, 91u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: ownerReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: ownerReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `portalImpl()` and selector `0x1b7cac5f`.
```solidity
function portalImpl() external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct portalImplCall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`portalImpl()`](portalImplCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct portalImplReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<portalImplCall> for UnderlyingRustTuple<'_> {
                fn from(value: portalImplCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for portalImplCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<portalImplReturn> for UnderlyingRustTuple<'_> {
                fn from(value: portalImplReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for portalImplReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for portalImplCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "portalImpl()";
            const SELECTOR: [u8; 4] = [27u8, 124u8, 172u8, 95u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: portalImplReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: portalImplReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `portalIsRegistered(address)` and selector `0xeb2347fd`.
```solidity
function portalIsRegistered(address portalAddress) external view returns (bool);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct portalIsRegisteredCall {
        #[allow(missing_docs)]
        pub portalAddress: alloy::sol_types::private::Address,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`portalIsRegistered(address)`](portalIsRegisteredCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct portalIsRegisteredReturn {
        #[allow(missing_docs)]
        pub _0: bool,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<portalIsRegisteredCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: portalIsRegisteredCall) -> Self {
                    (value.portalAddress,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for portalIsRegisteredCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { portalAddress: tuple.0 }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (bool,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<portalIsRegisteredReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: portalIsRegisteredReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for portalIsRegisteredReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for portalIsRegisteredCall {
            type Parameters<'a> = (alloy::sol_types::sol_data::Address,);
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = bool;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "portalIsRegistered(address)";
            const SELECTOR: [u8; 4] = [235u8, 35u8, 71u8, 253u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.portalAddress,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Bool as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: portalIsRegisteredReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: portalIsRegisteredReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `renounceOwnership()` and selector `0x715018a6`.
```solidity
function renounceOwnership() external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct renounceOwnershipCall;
    ///Container type for the return parameters of the [`renounceOwnership()`](renounceOwnershipCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct renounceOwnershipReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<renounceOwnershipCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: renounceOwnershipCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for renounceOwnershipCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<renounceOwnershipReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: renounceOwnershipReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for renounceOwnershipReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl renounceOwnershipReturn {
            fn _tokenize(
                &self,
            ) -> <renounceOwnershipCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for renounceOwnershipCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = renounceOwnershipReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "renounceOwnership()";
            const SELECTOR: [u8; 4] = [113u8, 80u8, 24u8, 166u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                renounceOwnershipReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `renounceRole(bytes32,address)` and selector `0x36568abe`.
```solidity
function renounceRole(bytes32 role, address callerConfirmation) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct renounceRoleCall {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub callerConfirmation: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`renounceRole(bytes32,address)`](renounceRoleCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct renounceRoleReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<renounceRoleCall> for UnderlyingRustTuple<'_> {
                fn from(value: renounceRoleCall) -> Self {
                    (value.role, value.callerConfirmation)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for renounceRoleCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        role: tuple.0,
                        callerConfirmation: tuple.1,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<renounceRoleReturn> for UnderlyingRustTuple<'_> {
                fn from(value: renounceRoleReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for renounceRoleReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl renounceRoleReturn {
            fn _tokenize(
                &self,
            ) -> <renounceRoleCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for renounceRoleCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = renounceRoleReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "renounceRole(bytes32,address)";
            const SELECTOR: [u8; 4] = [54u8, 86u8, 138u8, 190u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.role),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.callerConfirmation,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                renounceRoleReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `revokeRole(bytes32,address)` and selector `0xd547741f`.
```solidity
function revokeRole(bytes32 role, address account) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct revokeRoleCall {
        #[allow(missing_docs)]
        pub role: alloy::sol_types::private::FixedBytes<32>,
        #[allow(missing_docs)]
        pub account: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`revokeRole(bytes32,address)`](revokeRoleCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct revokeRoleReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::FixedBytes<32>,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<revokeRoleCall> for UnderlyingRustTuple<'_> {
                fn from(value: revokeRoleCall) -> Self {
                    (value.role, value.account)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for revokeRoleCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        role: tuple.0,
                        account: tuple.1,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<revokeRoleReturn> for UnderlyingRustTuple<'_> {
                fn from(value: revokeRoleReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for revokeRoleReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl revokeRoleReturn {
            fn _tokenize(
                &self,
            ) -> <revokeRoleCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for revokeRoleCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::FixedBytes<32>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = revokeRoleReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "revokeRole(bytes32,address)";
            const SELECTOR: [u8; 4] = [213u8, 71u8, 116u8, 31u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        32,
                    > as alloy_sol_types::SolType>::tokenize(&self.role),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.account,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                revokeRoleReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `solanaPortalImpl()` and selector `0x0c3148f5`.
```solidity
function solanaPortalImpl() external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct solanaPortalImplCall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`solanaPortalImpl()`](solanaPortalImplCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct solanaPortalImplReturn {
        #[allow(missing_docs)]
        pub _0: alloy::sol_types::private::Address,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<solanaPortalImplCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: solanaPortalImplCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for solanaPortalImplCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<solanaPortalImplReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: solanaPortalImplReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for solanaPortalImplReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for solanaPortalImplCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "solanaPortalImpl()";
            const SELECTOR: [u8; 4] = [12u8, 49u8, 72u8, 245u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                ()
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: solanaPortalImplReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: solanaPortalImplReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `supportsInterface(bytes4)` and selector `0x01ffc9a7`.
```solidity
function supportsInterface(bytes4 interfaceId) external view returns (bool);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct supportsInterfaceCall {
        #[allow(missing_docs)]
        pub interfaceId: alloy::sol_types::private::FixedBytes<4>,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`supportsInterface(bytes4)`](supportsInterfaceCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct supportsInterfaceReturn {
        #[allow(missing_docs)]
        pub _0: bool,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::FixedBytes<4>,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::FixedBytes<4>,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<supportsInterfaceCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: supportsInterfaceCall) -> Self {
                    (value.interfaceId,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for supportsInterfaceCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { interfaceId: tuple.0 }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (bool,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<supportsInterfaceReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: supportsInterfaceReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for supportsInterfaceReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for supportsInterfaceCall {
            type Parameters<'a> = (alloy::sol_types::sol_data::FixedBytes<4>,);
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = bool;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "supportsInterface(bytes4)";
            const SELECTOR: [u8; 4] = [1u8, 255u8, 201u8, 167u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::FixedBytes<
                        4,
                    > as alloy_sol_types::SolType>::tokenize(&self.interfaceId),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Bool as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: supportsInterfaceReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: supportsInterfaceReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `transferOwnership(address)` and selector `0xf2fde38b`.
```solidity
function transferOwnership(address newOwner) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct transferOwnershipCall {
        #[allow(missing_docs)]
        pub newOwner: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`transferOwnership(address)`](transferOwnershipCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct transferOwnershipReturn {}
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Address,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (alloy::sol_types::private::Address,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<transferOwnershipCall>
            for UnderlyingRustTuple<'_> {
                fn from(value: transferOwnershipCall) -> Self {
                    (value.newOwner,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for transferOwnershipCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { newOwner: tuple.0 }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = ();
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = ();
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<transferOwnershipReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: transferOwnershipReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for transferOwnershipReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl transferOwnershipReturn {
            fn _tokenize(
                &self,
            ) -> <transferOwnershipCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for transferOwnershipCall {
            type Parameters<'a> = (alloy::sol_types::sol_data::Address,);
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = transferOwnershipReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "transferOwnership(address)";
            const SELECTOR: [u8; 4] = [242u8, 253u8, 227u8, 139u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.newOwner,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                transferOwnershipReturn::_tokenize(ret)
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(Into::into)
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(Into::into)
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `updateConfig(address,address,address)` and selector `0x11c9a94d`.
```solidity
function updateConfig(address curvyVaultProxyAddress, address curvyAggregatorAlphaProxyAddress, address lifiDiamondAddress) external returns (bool);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct updateConfigCall {
        #[allow(missing_docs)]
        pub curvyVaultProxyAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub curvyAggregatorAlphaProxyAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub lifiDiamondAddress: alloy::sol_types::private::Address,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`updateConfig(address,address,address)`](updateConfigCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct updateConfigReturn {
        #[allow(missing_docs)]
        pub _0: bool,
    }
    #[allow(
        non_camel_case_types,
        non_snake_case,
        clippy::pub_underscore_fields,
        clippy::style
    )]
    const _: () = {
        use alloy::sol_types as alloy_sol_types;
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Address,
            );
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<updateConfigCall> for UnderlyingRustTuple<'_> {
                fn from(value: updateConfigCall) -> Self {
                    (
                        value.curvyVaultProxyAddress,
                        value.curvyAggregatorAlphaProxyAddress,
                        value.lifiDiamondAddress,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for updateConfigCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        curvyVaultProxyAddress: tuple.0,
                        curvyAggregatorAlphaProxyAddress: tuple.1,
                        lifiDiamondAddress: tuple.2,
                    }
                }
            }
        }
        {
            #[doc(hidden)]
            #[allow(dead_code)]
            type UnderlyingSolTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (bool,);
            #[cfg(test)]
            #[allow(dead_code, unreachable_patterns)]
            fn _type_assertion(
                _t: alloy_sol_types::private::AssertTypeEq<UnderlyingRustTuple>,
            ) {
                match _t {
                    alloy_sol_types::private::AssertTypeEq::<
                        <UnderlyingSolTuple as alloy_sol_types::SolType>::RustType,
                    >(_) => {}
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<updateConfigReturn> for UnderlyingRustTuple<'_> {
                fn from(value: updateConfigReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for updateConfigReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for updateConfigCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = bool;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Bool,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "updateConfig(address,address,address)";
            const SELECTOR: [u8; 4] = [17u8, 201u8, 169u8, 77u8];
            #[inline]
            fn new<'a>(
                tuple: <Self::Parameters<'a> as alloy_sol_types::SolType>::RustType,
            ) -> Self {
                tuple.into()
            }
            #[inline]
            fn tokenize(&self) -> Self::Token<'_> {
                (
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.curvyVaultProxyAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.curvyAggregatorAlphaProxyAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.lifiDiamondAddress,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                (
                    <alloy::sol_types::sol_data::Bool as alloy_sol_types::SolType>::tokenize(
                        ret,
                    ),
                )
            }
            #[inline]
            fn abi_decode_returns(data: &[u8]) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence(data)
                    .map(|r| {
                        let r: updateConfigReturn = r.into();
                        r._0
                    })
            }
            #[inline]
            fn abi_decode_returns_validate(
                data: &[u8],
            ) -> alloy_sol_types::Result<Self::Return> {
                <Self::ReturnTuple<
                    '_,
                > as alloy_sol_types::SolType>::abi_decode_sequence_validate(data)
                    .map(|r| {
                        let r: updateConfigReturn = r.into();
                        r._0
                    })
            }
        }
    };
    ///Container for all the [`PortalFactory`](self) function calls.
    #[derive(Clone)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive()]
    pub enum PortalFactoryCalls {
        #[allow(missing_docs)]
        AUTHORITY_ROLE(AUTHORITY_ROLECall),
        #[allow(missing_docs)]
        DEFAULT_ADMIN_ROLE(DEFAULT_ADMIN_ROLECall),
        #[allow(missing_docs)]
        OPERATOR_ROLE(OPERATOR_ROLECall),
        #[allow(missing_docs)]
        deployEntryBridgePortal(deployEntryBridgePortalCall),
        #[allow(missing_docs)]
        deployExitBridgePortal(deployExitBridgePortalCall),
        #[allow(missing_docs)]
        deployRecoveryEntryPortal(deployRecoveryEntryPortalCall),
        #[allow(missing_docs)]
        deployRecoveryExitPortal(deployRecoveryExitPortalCall),
        #[allow(missing_docs)]
        deployShieldPortal(deployShieldPortalCall),
        #[allow(missing_docs)]
        deploySolanaExitBridgePortal(deploySolanaExitBridgePortalCall),
        #[allow(missing_docs)]
        deploySolanaRecoveryExitPortal(deploySolanaRecoveryExitPortalCall),
        #[allow(missing_docs)]
        getEntryPortalAddress(getEntryPortalAddressCall),
        #[allow(missing_docs)]
        getExitPortalAddress(getExitPortalAddressCall),
        #[allow(missing_docs)]
        getRoleAdmin(getRoleAdminCall),
        #[allow(missing_docs)]
        getSolanaExitPortalAddress(getSolanaExitPortalAddressCall),
        #[allow(missing_docs)]
        grantRole(grantRoleCall),
        #[allow(missing_docs)]
        hasRole(hasRoleCall),
        #[allow(missing_docs)]
        owner(ownerCall),
        #[allow(missing_docs)]
        portalImpl(portalImplCall),
        #[allow(missing_docs)]
        portalIsRegistered(portalIsRegisteredCall),
        #[allow(missing_docs)]
        renounceOwnership(renounceOwnershipCall),
        #[allow(missing_docs)]
        renounceRole(renounceRoleCall),
        #[allow(missing_docs)]
        revokeRole(revokeRoleCall),
        #[allow(missing_docs)]
        solanaPortalImpl(solanaPortalImplCall),
        #[allow(missing_docs)]
        supportsInterface(supportsInterfaceCall),
        #[allow(missing_docs)]
        transferOwnership(transferOwnershipCall),
        #[allow(missing_docs)]
        updateConfig(updateConfigCall),
    }
    impl PortalFactoryCalls {
        /// All the selectors of this enum.
        ///
        /// Note that the selectors might not be in the same order as the variants.
        /// No guarantees are made about the order of the selectors.
        ///
        /// Prefer using `SolInterface` methods instead.
        pub const SELECTORS: &'static [[u8; 4usize]] = &[
            [1u8, 255u8, 201u8, 167u8],
            [12u8, 49u8, 72u8, 245u8],
            [17u8, 201u8, 169u8, 77u8],
            [27u8, 124u8, 172u8, 95u8],
            [36u8, 138u8, 156u8, 163u8],
            [42u8, 51u8, 207u8, 46u8],
            [47u8, 47u8, 241u8, 93u8],
            [47u8, 56u8, 25u8, 230u8],
            [54u8, 86u8, 138u8, 190u8],
            [74u8, 63u8, 186u8, 14u8],
            [83u8, 7u8, 11u8, 85u8],
            [94u8, 139u8, 149u8, 210u8],
            [102u8, 233u8, 59u8, 140u8],
            [113u8, 80u8, 24u8, 166u8],
            [132u8, 140u8, 159u8, 130u8],
            [141u8, 165u8, 203u8, 91u8],
            [145u8, 209u8, 72u8, 84u8],
            [162u8, 23u8, 253u8, 223u8],
            [182u8, 75u8, 42u8, 138u8],
            [188u8, 52u8, 136u8, 198u8],
            [213u8, 71u8, 116u8, 31u8],
            [216u8, 5u8, 19u8, 181u8],
            [225u8, 108u8, 168u8, 149u8],
            [235u8, 35u8, 71u8, 253u8],
            [242u8, 253u8, 227u8, 139u8],
            [245u8, 181u8, 65u8, 166u8],
        ];
        /// The names of the variants in the same order as `SELECTORS`.
        pub const VARIANT_NAMES: &'static [&'static str] = &[
            ::core::stringify!(supportsInterface),
            ::core::stringify!(solanaPortalImpl),
            ::core::stringify!(updateConfig),
            ::core::stringify!(portalImpl),
            ::core::stringify!(getRoleAdmin),
            ::core::stringify!(deployExitBridgePortal),
            ::core::stringify!(grantRole),
            ::core::stringify!(deploySolanaRecoveryExitPortal),
            ::core::stringify!(renounceRole),
            ::core::stringify!(AUTHORITY_ROLE),
            ::core::stringify!(deployRecoveryEntryPortal),
            ::core::stringify!(getEntryPortalAddress),
            ::core::stringify!(deployRecoveryExitPortal),
            ::core::stringify!(renounceOwnership),
            ::core::stringify!(deployEntryBridgePortal),
            ::core::stringify!(owner),
            ::core::stringify!(hasRole),
            ::core::stringify!(DEFAULT_ADMIN_ROLE),
            ::core::stringify!(deployShieldPortal),
            ::core::stringify!(deploySolanaExitBridgePortal),
            ::core::stringify!(revokeRole),
            ::core::stringify!(getSolanaExitPortalAddress),
            ::core::stringify!(getExitPortalAddress),
            ::core::stringify!(portalIsRegistered),
            ::core::stringify!(transferOwnership),
            ::core::stringify!(OPERATOR_ROLE),
        ];
        /// The signatures in the same order as `SELECTORS`.
        pub const SIGNATURES: &'static [&'static str] = &[
            <supportsInterfaceCall as alloy_sol_types::SolCall>::SIGNATURE,
            <solanaPortalImplCall as alloy_sol_types::SolCall>::SIGNATURE,
            <updateConfigCall as alloy_sol_types::SolCall>::SIGNATURE,
            <portalImplCall as alloy_sol_types::SolCall>::SIGNATURE,
            <getRoleAdminCall as alloy_sol_types::SolCall>::SIGNATURE,
            <deployExitBridgePortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <grantRoleCall as alloy_sol_types::SolCall>::SIGNATURE,
            <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <renounceRoleCall as alloy_sol_types::SolCall>::SIGNATURE,
            <AUTHORITY_ROLECall as alloy_sol_types::SolCall>::SIGNATURE,
            <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <getEntryPortalAddressCall as alloy_sol_types::SolCall>::SIGNATURE,
            <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <renounceOwnershipCall as alloy_sol_types::SolCall>::SIGNATURE,
            <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <ownerCall as alloy_sol_types::SolCall>::SIGNATURE,
            <hasRoleCall as alloy_sol_types::SolCall>::SIGNATURE,
            <DEFAULT_ADMIN_ROLECall as alloy_sol_types::SolCall>::SIGNATURE,
            <deployShieldPortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::SIGNATURE,
            <revokeRoleCall as alloy_sol_types::SolCall>::SIGNATURE,
            <getSolanaExitPortalAddressCall as alloy_sol_types::SolCall>::SIGNATURE,
            <getExitPortalAddressCall as alloy_sol_types::SolCall>::SIGNATURE,
            <portalIsRegisteredCall as alloy_sol_types::SolCall>::SIGNATURE,
            <transferOwnershipCall as alloy_sol_types::SolCall>::SIGNATURE,
            <OPERATOR_ROLECall as alloy_sol_types::SolCall>::SIGNATURE,
        ];
        /// Returns the signature for the given selector, if known.
        #[inline]
        pub fn signature_by_selector(
            selector: [u8; 4usize],
        ) -> ::core::option::Option<&'static str> {
            match Self::SELECTORS.binary_search(&selector) {
                ::core::result::Result::Ok(idx) => {
                    ::core::option::Option::Some(Self::SIGNATURES[idx])
                }
                ::core::result::Result::Err(_) => ::core::option::Option::None,
            }
        }
        /// Returns the enum variant name for the given selector, if known.
        #[inline]
        pub fn name_by_selector(
            selector: [u8; 4usize],
        ) -> ::core::option::Option<&'static str> {
            let sig = Self::signature_by_selector(selector)?;
            sig.split_once('(').map(|(name, _)| name)
        }
    }
    #[automatically_derived]
    impl alloy_sol_types::SolInterface for PortalFactoryCalls {
        const NAME: &'static str = "PortalFactoryCalls";
        const MIN_DATA_LENGTH: usize = 0usize;
        const COUNT: usize = 26usize;
        #[inline]
        fn selector(&self) -> [u8; 4] {
            match self {
                Self::AUTHORITY_ROLE(_) => {
                    <AUTHORITY_ROLECall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::DEFAULT_ADMIN_ROLE(_) => {
                    <DEFAULT_ADMIN_ROLECall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::OPERATOR_ROLE(_) => {
                    <OPERATOR_ROLECall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deployEntryBridgePortal(_) => {
                    <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deployExitBridgePortal(_) => {
                    <deployExitBridgePortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deployRecoveryEntryPortal(_) => {
                    <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deployRecoveryExitPortal(_) => {
                    <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deployShieldPortal(_) => {
                    <deployShieldPortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deploySolanaExitBridgePortal(_) => {
                    <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::deploySolanaRecoveryExitPortal(_) => {
                    <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::getEntryPortalAddress(_) => {
                    <getEntryPortalAddressCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::getExitPortalAddress(_) => {
                    <getExitPortalAddressCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::getRoleAdmin(_) => {
                    <getRoleAdminCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::getSolanaExitPortalAddress(_) => {
                    <getSolanaExitPortalAddressCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::grantRole(_) => {
                    <grantRoleCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::hasRole(_) => <hasRoleCall as alloy_sol_types::SolCall>::SELECTOR,
                Self::owner(_) => <ownerCall as alloy_sol_types::SolCall>::SELECTOR,
                Self::portalImpl(_) => {
                    <portalImplCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::portalIsRegistered(_) => {
                    <portalIsRegisteredCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::renounceOwnership(_) => {
                    <renounceOwnershipCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::renounceRole(_) => {
                    <renounceRoleCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::revokeRole(_) => {
                    <revokeRoleCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::solanaPortalImpl(_) => {
                    <solanaPortalImplCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::supportsInterface(_) => {
                    <supportsInterfaceCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::transferOwnership(_) => {
                    <transferOwnershipCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::updateConfig(_) => {
                    <updateConfigCall as alloy_sol_types::SolCall>::SELECTOR
                }
            }
        }
        #[inline]
        fn selector_at(i: usize) -> ::core::option::Option<[u8; 4]> {
            Self::SELECTORS.get(i).copied()
        }
        #[inline]
        fn valid_selector(selector: [u8; 4]) -> bool {
            Self::SELECTORS.binary_search(&selector).is_ok()
        }
        #[inline]
        #[allow(non_snake_case)]
        fn abi_decode_raw(
            selector: [u8; 4],
            data: &[u8],
        ) -> alloy_sol_types::Result<Self> {
            static DECODE_SHIMS: &[fn(
                &[u8],
            ) -> alloy_sol_types::Result<PortalFactoryCalls>] = &[
                {
                    fn supportsInterface(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <supportsInterfaceCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::supportsInterface)
                    }
                    supportsInterface
                },
                {
                    fn solanaPortalImpl(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <solanaPortalImplCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::solanaPortalImpl)
                    }
                    solanaPortalImpl
                },
                {
                    fn updateConfig(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <updateConfigCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::updateConfig)
                    }
                    updateConfig
                },
                {
                    fn portalImpl(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <portalImplCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::portalImpl)
                    }
                    portalImpl
                },
                {
                    fn getRoleAdmin(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getRoleAdminCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::getRoleAdmin)
                    }
                    getRoleAdmin
                },
                {
                    fn deployExitBridgePortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployExitBridgePortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deployExitBridgePortal)
                    }
                    deployExitBridgePortal
                },
                {
                    fn grantRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <grantRoleCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalFactoryCalls::grantRole)
                    }
                    grantRole
                },
                {
                    fn deploySolanaRecoveryExitPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deploySolanaRecoveryExitPortal)
                    }
                    deploySolanaRecoveryExitPortal
                },
                {
                    fn renounceRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <renounceRoleCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::renounceRole)
                    }
                    renounceRole
                },
                {
                    fn AUTHORITY_ROLE(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <AUTHORITY_ROLECall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::AUTHORITY_ROLE)
                    }
                    AUTHORITY_ROLE
                },
                {
                    fn deployRecoveryEntryPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deployRecoveryEntryPortal)
                    }
                    deployRecoveryEntryPortal
                },
                {
                    fn getEntryPortalAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getEntryPortalAddressCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::getEntryPortalAddress)
                    }
                    getEntryPortalAddress
                },
                {
                    fn deployRecoveryExitPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deployRecoveryExitPortal)
                    }
                    deployRecoveryExitPortal
                },
                {
                    fn renounceOwnership(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <renounceOwnershipCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::renounceOwnership)
                    }
                    renounceOwnership
                },
                {
                    fn deployEntryBridgePortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deployEntryBridgePortal)
                    }
                    deployEntryBridgePortal
                },
                {
                    fn owner(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <ownerCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalFactoryCalls::owner)
                    }
                    owner
                },
                {
                    fn hasRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <hasRoleCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalFactoryCalls::hasRole)
                    }
                    hasRole
                },
                {
                    fn DEFAULT_ADMIN_ROLE(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <DEFAULT_ADMIN_ROLECall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::DEFAULT_ADMIN_ROLE)
                    }
                    DEFAULT_ADMIN_ROLE
                },
                {
                    fn deployShieldPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployShieldPortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deployShieldPortal)
                    }
                    deployShieldPortal
                },
                {
                    fn deploySolanaExitBridgePortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::deploySolanaExitBridgePortal)
                    }
                    deploySolanaExitBridgePortal
                },
                {
                    fn revokeRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <revokeRoleCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::revokeRole)
                    }
                    revokeRole
                },
                {
                    fn getSolanaExitPortalAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getSolanaExitPortalAddressCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::getSolanaExitPortalAddress)
                    }
                    getSolanaExitPortalAddress
                },
                {
                    fn getExitPortalAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getExitPortalAddressCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::getExitPortalAddress)
                    }
                    getExitPortalAddress
                },
                {
                    fn portalIsRegistered(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <portalIsRegisteredCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::portalIsRegistered)
                    }
                    portalIsRegistered
                },
                {
                    fn transferOwnership(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <transferOwnershipCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::transferOwnership)
                    }
                    transferOwnership
                },
                {
                    fn OPERATOR_ROLE(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <OPERATOR_ROLECall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryCalls::OPERATOR_ROLE)
                    }
                    OPERATOR_ROLE
                },
            ];
            let Ok(idx) = Self::SELECTORS.binary_search(&selector) else {
                return Err(
                    alloy_sol_types::Error::unknown_selector(
                        <Self as alloy_sol_types::SolInterface>::NAME,
                        selector,
                    ),
                );
            };
            DECODE_SHIMS[idx](data)
        }
        #[inline]
        #[allow(non_snake_case)]
        fn abi_decode_raw_validate(
            selector: [u8; 4],
            data: &[u8],
        ) -> alloy_sol_types::Result<Self> {
            static DECODE_VALIDATE_SHIMS: &[fn(
                &[u8],
            ) -> alloy_sol_types::Result<PortalFactoryCalls>] = &[
                {
                    fn supportsInterface(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <supportsInterfaceCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::supportsInterface)
                    }
                    supportsInterface
                },
                {
                    fn solanaPortalImpl(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <solanaPortalImplCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::solanaPortalImpl)
                    }
                    solanaPortalImpl
                },
                {
                    fn updateConfig(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <updateConfigCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::updateConfig)
                    }
                    updateConfig
                },
                {
                    fn portalImpl(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <portalImplCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::portalImpl)
                    }
                    portalImpl
                },
                {
                    fn getRoleAdmin(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getRoleAdminCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::getRoleAdmin)
                    }
                    getRoleAdmin
                },
                {
                    fn deployExitBridgePortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployExitBridgePortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deployExitBridgePortal)
                    }
                    deployExitBridgePortal
                },
                {
                    fn grantRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <grantRoleCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::grantRole)
                    }
                    grantRole
                },
                {
                    fn deploySolanaRecoveryExitPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deploySolanaRecoveryExitPortal)
                    }
                    deploySolanaRecoveryExitPortal
                },
                {
                    fn renounceRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <renounceRoleCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::renounceRole)
                    }
                    renounceRole
                },
                {
                    fn AUTHORITY_ROLE(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <AUTHORITY_ROLECall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::AUTHORITY_ROLE)
                    }
                    AUTHORITY_ROLE
                },
                {
                    fn deployRecoveryEntryPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deployRecoveryEntryPortal)
                    }
                    deployRecoveryEntryPortal
                },
                {
                    fn getEntryPortalAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getEntryPortalAddressCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::getEntryPortalAddress)
                    }
                    getEntryPortalAddress
                },
                {
                    fn deployRecoveryExitPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deployRecoveryExitPortal)
                    }
                    deployRecoveryExitPortal
                },
                {
                    fn renounceOwnership(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <renounceOwnershipCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::renounceOwnership)
                    }
                    renounceOwnership
                },
                {
                    fn deployEntryBridgePortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deployEntryBridgePortal)
                    }
                    deployEntryBridgePortal
                },
                {
                    fn owner(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <ownerCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::owner)
                    }
                    owner
                },
                {
                    fn hasRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <hasRoleCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::hasRole)
                    }
                    hasRole
                },
                {
                    fn DEFAULT_ADMIN_ROLE(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <DEFAULT_ADMIN_ROLECall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::DEFAULT_ADMIN_ROLE)
                    }
                    DEFAULT_ADMIN_ROLE
                },
                {
                    fn deployShieldPortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deployShieldPortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deployShieldPortal)
                    }
                    deployShieldPortal
                },
                {
                    fn deploySolanaExitBridgePortal(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::deploySolanaExitBridgePortal)
                    }
                    deploySolanaExitBridgePortal
                },
                {
                    fn revokeRole(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <revokeRoleCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::revokeRole)
                    }
                    revokeRole
                },
                {
                    fn getSolanaExitPortalAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getSolanaExitPortalAddressCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::getSolanaExitPortalAddress)
                    }
                    getSolanaExitPortalAddress
                },
                {
                    fn getExitPortalAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <getExitPortalAddressCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::getExitPortalAddress)
                    }
                    getExitPortalAddress
                },
                {
                    fn portalIsRegistered(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <portalIsRegisteredCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::portalIsRegistered)
                    }
                    portalIsRegistered
                },
                {
                    fn transferOwnership(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <transferOwnershipCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::transferOwnership)
                    }
                    transferOwnership
                },
                {
                    fn OPERATOR_ROLE(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryCalls> {
                        <OPERATOR_ROLECall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryCalls::OPERATOR_ROLE)
                    }
                    OPERATOR_ROLE
                },
            ];
            let Ok(idx) = Self::SELECTORS.binary_search(&selector) else {
                return Err(
                    alloy_sol_types::Error::unknown_selector(
                        <Self as alloy_sol_types::SolInterface>::NAME,
                        selector,
                    ),
                );
            };
            DECODE_VALIDATE_SHIMS[idx](data)
        }
        #[inline]
        fn abi_encoded_size(&self) -> usize {
            match self {
                Self::AUTHORITY_ROLE(inner) => {
                    <AUTHORITY_ROLECall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::DEFAULT_ADMIN_ROLE(inner) => {
                    <DEFAULT_ADMIN_ROLECall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::OPERATOR_ROLE(inner) => {
                    <OPERATOR_ROLECall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deployEntryBridgePortal(inner) => {
                    <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deployExitBridgePortal(inner) => {
                    <deployExitBridgePortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deployRecoveryEntryPortal(inner) => {
                    <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deployRecoveryExitPortal(inner) => {
                    <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deployShieldPortal(inner) => {
                    <deployShieldPortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deploySolanaExitBridgePortal(inner) => {
                    <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::deploySolanaRecoveryExitPortal(inner) => {
                    <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::getEntryPortalAddress(inner) => {
                    <getEntryPortalAddressCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::getExitPortalAddress(inner) => {
                    <getExitPortalAddressCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::getRoleAdmin(inner) => {
                    <getRoleAdminCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::getSolanaExitPortalAddress(inner) => {
                    <getSolanaExitPortalAddressCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::grantRole(inner) => {
                    <grantRoleCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::hasRole(inner) => {
                    <hasRoleCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::owner(inner) => {
                    <ownerCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::portalImpl(inner) => {
                    <portalImplCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::portalIsRegistered(inner) => {
                    <portalIsRegisteredCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::renounceOwnership(inner) => {
                    <renounceOwnershipCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::renounceRole(inner) => {
                    <renounceRoleCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::revokeRole(inner) => {
                    <revokeRoleCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::solanaPortalImpl(inner) => {
                    <solanaPortalImplCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::supportsInterface(inner) => {
                    <supportsInterfaceCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::transferOwnership(inner) => {
                    <transferOwnershipCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::updateConfig(inner) => {
                    <updateConfigCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
            }
        }
        #[inline]
        fn abi_encode_raw(&self, out: &mut alloy_sol_types::private::Vec<u8>) {
            match self {
                Self::AUTHORITY_ROLE(inner) => {
                    <AUTHORITY_ROLECall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::DEFAULT_ADMIN_ROLE(inner) => {
                    <DEFAULT_ADMIN_ROLECall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::OPERATOR_ROLE(inner) => {
                    <OPERATOR_ROLECall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deployEntryBridgePortal(inner) => {
                    <deployEntryBridgePortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deployExitBridgePortal(inner) => {
                    <deployExitBridgePortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deployRecoveryEntryPortal(inner) => {
                    <deployRecoveryEntryPortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deployRecoveryExitPortal(inner) => {
                    <deployRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deployShieldPortal(inner) => {
                    <deployShieldPortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deploySolanaExitBridgePortal(inner) => {
                    <deploySolanaExitBridgePortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::deploySolanaRecoveryExitPortal(inner) => {
                    <deploySolanaRecoveryExitPortalCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::getEntryPortalAddress(inner) => {
                    <getEntryPortalAddressCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::getExitPortalAddress(inner) => {
                    <getExitPortalAddressCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::getRoleAdmin(inner) => {
                    <getRoleAdminCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::getSolanaExitPortalAddress(inner) => {
                    <getSolanaExitPortalAddressCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::grantRole(inner) => {
                    <grantRoleCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::hasRole(inner) => {
                    <hasRoleCall as alloy_sol_types::SolCall>::abi_encode_raw(inner, out)
                }
                Self::owner(inner) => {
                    <ownerCall as alloy_sol_types::SolCall>::abi_encode_raw(inner, out)
                }
                Self::portalImpl(inner) => {
                    <portalImplCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::portalIsRegistered(inner) => {
                    <portalIsRegisteredCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::renounceOwnership(inner) => {
                    <renounceOwnershipCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::renounceRole(inner) => {
                    <renounceRoleCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::revokeRole(inner) => {
                    <revokeRoleCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::solanaPortalImpl(inner) => {
                    <solanaPortalImplCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::supportsInterface(inner) => {
                    <supportsInterfaceCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::transferOwnership(inner) => {
                    <transferOwnershipCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::updateConfig(inner) => {
                    <updateConfigCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
            }
        }
    }
    ///Container for all the [`PortalFactory`](self) custom errors.
    #[derive(Clone)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub enum PortalFactoryErrors {
        #[allow(missing_docs)]
        AccessControlBadConfirmation(AccessControlBadConfirmation),
        #[allow(missing_docs)]
        AccessControlUnauthorizedAccount(AccessControlUnauthorizedAccount),
        #[allow(missing_docs)]
        AmountMismatch(AmountMismatch),
        #[allow(missing_docs)]
        DeploymentFailed(DeploymentFailed),
        #[allow(missing_docs)]
        FailedDeployment(FailedDeployment),
        #[allow(missing_docs)]
        InsufficientBalance(InsufficientBalance),
        #[allow(missing_docs)]
        InvalidLiFiDestinationChain(InvalidLiFiDestinationChain),
        #[allow(missing_docs)]
        InvalidLiFiReceiver(InvalidLiFiReceiver),
        #[allow(missing_docs)]
        OwnableInvalidOwner(OwnableInvalidOwner),
        #[allow(missing_docs)]
        OwnableUnauthorizedAccount(OwnableUnauthorizedAccount),
        #[allow(missing_docs)]
        UnsupportedBridging(UnsupportedBridging),
        #[allow(missing_docs)]
        UnsupportedShielding(UnsupportedShielding),
    }
    impl PortalFactoryErrors {
        /// All the selectors of this enum.
        ///
        /// Note that the selectors might not be in the same order as the variants.
        /// No guarantees are made about the order of the selectors.
        ///
        /// Prefer using `SolInterface` methods instead.
        pub const SELECTORS: &'static [[u8; 4usize]] = &[
            [17u8, 140u8, 218u8, 167u8],
            [30u8, 79u8, 189u8, 247u8],
            [48u8, 17u8, 100u8, 37u8],
            [67u8, 127u8, 58u8, 195u8],
            [82u8, 54u8, 96u8, 247u8],
            [85u8, 233u8, 123u8, 13u8],
            [102u8, 151u8, 178u8, 50u8],
            [137u8, 218u8, 113u8, 79u8],
            [151u8, 201u8, 27u8, 105u8],
            [176u8, 110u8, 191u8, 61u8],
            [207u8, 71u8, 145u8, 129u8],
            [226u8, 81u8, 125u8, 63u8],
        ];
        /// The names of the variants in the same order as `SELECTORS`.
        pub const VARIANT_NAMES: &'static [&'static str] = &[
            ::core::stringify!(OwnableUnauthorizedAccount),
            ::core::stringify!(OwnableInvalidOwner),
            ::core::stringify!(DeploymentFailed),
            ::core::stringify!(UnsupportedBridging),
            ::core::stringify!(InvalidLiFiReceiver),
            ::core::stringify!(AmountMismatch),
            ::core::stringify!(AccessControlBadConfirmation),
            ::core::stringify!(UnsupportedShielding),
            ::core::stringify!(InvalidLiFiDestinationChain),
            ::core::stringify!(FailedDeployment),
            ::core::stringify!(InsufficientBalance),
            ::core::stringify!(AccessControlUnauthorizedAccount),
        ];
        /// The signatures in the same order as `SELECTORS`.
        pub const SIGNATURES: &'static [&'static str] = &[
            <OwnableUnauthorizedAccount as alloy_sol_types::SolError>::SIGNATURE,
            <OwnableInvalidOwner as alloy_sol_types::SolError>::SIGNATURE,
            <DeploymentFailed as alloy_sol_types::SolError>::SIGNATURE,
            <UnsupportedBridging as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidLiFiReceiver as alloy_sol_types::SolError>::SIGNATURE,
            <AmountMismatch as alloy_sol_types::SolError>::SIGNATURE,
            <AccessControlBadConfirmation as alloy_sol_types::SolError>::SIGNATURE,
            <UnsupportedShielding as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidLiFiDestinationChain as alloy_sol_types::SolError>::SIGNATURE,
            <FailedDeployment as alloy_sol_types::SolError>::SIGNATURE,
            <InsufficientBalance as alloy_sol_types::SolError>::SIGNATURE,
            <AccessControlUnauthorizedAccount as alloy_sol_types::SolError>::SIGNATURE,
        ];
        /// Returns the signature for the given selector, if known.
        #[inline]
        pub fn signature_by_selector(
            selector: [u8; 4usize],
        ) -> ::core::option::Option<&'static str> {
            match Self::SELECTORS.binary_search(&selector) {
                ::core::result::Result::Ok(idx) => {
                    ::core::option::Option::Some(Self::SIGNATURES[idx])
                }
                ::core::result::Result::Err(_) => ::core::option::Option::None,
            }
        }
        /// Returns the enum variant name for the given selector, if known.
        #[inline]
        pub fn name_by_selector(
            selector: [u8; 4usize],
        ) -> ::core::option::Option<&'static str> {
            let sig = Self::signature_by_selector(selector)?;
            sig.split_once('(').map(|(name, _)| name)
        }
    }
    #[automatically_derived]
    impl alloy_sol_types::SolInterface for PortalFactoryErrors {
        const NAME: &'static str = "PortalFactoryErrors";
        const MIN_DATA_LENGTH: usize = 0usize;
        const COUNT: usize = 12usize;
        #[inline]
        fn selector(&self) -> [u8; 4] {
            match self {
                Self::AccessControlBadConfirmation(_) => {
                    <AccessControlBadConfirmation as alloy_sol_types::SolError>::SELECTOR
                }
                Self::AccessControlUnauthorizedAccount(_) => {
                    <AccessControlUnauthorizedAccount as alloy_sol_types::SolError>::SELECTOR
                }
                Self::AmountMismatch(_) => {
                    <AmountMismatch as alloy_sol_types::SolError>::SELECTOR
                }
                Self::DeploymentFailed(_) => {
                    <DeploymentFailed as alloy_sol_types::SolError>::SELECTOR
                }
                Self::FailedDeployment(_) => {
                    <FailedDeployment as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InsufficientBalance(_) => {
                    <InsufficientBalance as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidLiFiDestinationChain(_) => {
                    <InvalidLiFiDestinationChain as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidLiFiReceiver(_) => {
                    <InvalidLiFiReceiver as alloy_sol_types::SolError>::SELECTOR
                }
                Self::OwnableInvalidOwner(_) => {
                    <OwnableInvalidOwner as alloy_sol_types::SolError>::SELECTOR
                }
                Self::OwnableUnauthorizedAccount(_) => {
                    <OwnableUnauthorizedAccount as alloy_sol_types::SolError>::SELECTOR
                }
                Self::UnsupportedBridging(_) => {
                    <UnsupportedBridging as alloy_sol_types::SolError>::SELECTOR
                }
                Self::UnsupportedShielding(_) => {
                    <UnsupportedShielding as alloy_sol_types::SolError>::SELECTOR
                }
            }
        }
        #[inline]
        fn selector_at(i: usize) -> ::core::option::Option<[u8; 4]> {
            Self::SELECTORS.get(i).copied()
        }
        #[inline]
        fn valid_selector(selector: [u8; 4]) -> bool {
            Self::SELECTORS.binary_search(&selector).is_ok()
        }
        #[inline]
        #[allow(non_snake_case)]
        fn abi_decode_raw(
            selector: [u8; 4],
            data: &[u8],
        ) -> alloy_sol_types::Result<Self> {
            static DECODE_SHIMS: &[fn(
                &[u8],
            ) -> alloy_sol_types::Result<PortalFactoryErrors>] = &[
                {
                    fn OwnableUnauthorizedAccount(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <OwnableUnauthorizedAccount as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::OwnableUnauthorizedAccount)
                    }
                    OwnableUnauthorizedAccount
                },
                {
                    fn OwnableInvalidOwner(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <OwnableInvalidOwner as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::OwnableInvalidOwner)
                    }
                    OwnableInvalidOwner
                },
                {
                    fn DeploymentFailed(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <DeploymentFailed as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::DeploymentFailed)
                    }
                    DeploymentFailed
                },
                {
                    fn UnsupportedBridging(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <UnsupportedBridging as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::UnsupportedBridging)
                    }
                    UnsupportedBridging
                },
                {
                    fn InvalidLiFiReceiver(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <InvalidLiFiReceiver as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::InvalidLiFiReceiver)
                    }
                    InvalidLiFiReceiver
                },
                {
                    fn AmountMismatch(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <AmountMismatch as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::AmountMismatch)
                    }
                    AmountMismatch
                },
                {
                    fn AccessControlBadConfirmation(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <AccessControlBadConfirmation as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::AccessControlBadConfirmation)
                    }
                    AccessControlBadConfirmation
                },
                {
                    fn UnsupportedShielding(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <UnsupportedShielding as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::UnsupportedShielding)
                    }
                    UnsupportedShielding
                },
                {
                    fn InvalidLiFiDestinationChain(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <InvalidLiFiDestinationChain as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::InvalidLiFiDestinationChain)
                    }
                    InvalidLiFiDestinationChain
                },
                {
                    fn FailedDeployment(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <FailedDeployment as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::FailedDeployment)
                    }
                    FailedDeployment
                },
                {
                    fn InsufficientBalance(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <InsufficientBalance as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::InsufficientBalance)
                    }
                    InsufficientBalance
                },
                {
                    fn AccessControlUnauthorizedAccount(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <AccessControlUnauthorizedAccount as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalFactoryErrors::AccessControlUnauthorizedAccount)
                    }
                    AccessControlUnauthorizedAccount
                },
            ];
            let Ok(idx) = Self::SELECTORS.binary_search(&selector) else {
                return Err(
                    alloy_sol_types::Error::unknown_selector(
                        <Self as alloy_sol_types::SolInterface>::NAME,
                        selector,
                    ),
                );
            };
            DECODE_SHIMS[idx](data)
        }
        #[inline]
        #[allow(non_snake_case)]
        fn abi_decode_raw_validate(
            selector: [u8; 4],
            data: &[u8],
        ) -> alloy_sol_types::Result<Self> {
            static DECODE_VALIDATE_SHIMS: &[fn(
                &[u8],
            ) -> alloy_sol_types::Result<PortalFactoryErrors>] = &[
                {
                    fn OwnableUnauthorizedAccount(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <OwnableUnauthorizedAccount as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::OwnableUnauthorizedAccount)
                    }
                    OwnableUnauthorizedAccount
                },
                {
                    fn OwnableInvalidOwner(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <OwnableInvalidOwner as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::OwnableInvalidOwner)
                    }
                    OwnableInvalidOwner
                },
                {
                    fn DeploymentFailed(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <DeploymentFailed as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::DeploymentFailed)
                    }
                    DeploymentFailed
                },
                {
                    fn UnsupportedBridging(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <UnsupportedBridging as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::UnsupportedBridging)
                    }
                    UnsupportedBridging
                },
                {
                    fn InvalidLiFiReceiver(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <InvalidLiFiReceiver as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::InvalidLiFiReceiver)
                    }
                    InvalidLiFiReceiver
                },
                {
                    fn AmountMismatch(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <AmountMismatch as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::AmountMismatch)
                    }
                    AmountMismatch
                },
                {
                    fn AccessControlBadConfirmation(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <AccessControlBadConfirmation as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::AccessControlBadConfirmation)
                    }
                    AccessControlBadConfirmation
                },
                {
                    fn UnsupportedShielding(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <UnsupportedShielding as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::UnsupportedShielding)
                    }
                    UnsupportedShielding
                },
                {
                    fn InvalidLiFiDestinationChain(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <InvalidLiFiDestinationChain as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::InvalidLiFiDestinationChain)
                    }
                    InvalidLiFiDestinationChain
                },
                {
                    fn FailedDeployment(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <FailedDeployment as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::FailedDeployment)
                    }
                    FailedDeployment
                },
                {
                    fn InsufficientBalance(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <InsufficientBalance as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::InsufficientBalance)
                    }
                    InsufficientBalance
                },
                {
                    fn AccessControlUnauthorizedAccount(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalFactoryErrors> {
                        <AccessControlUnauthorizedAccount as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalFactoryErrors::AccessControlUnauthorizedAccount)
                    }
                    AccessControlUnauthorizedAccount
                },
            ];
            let Ok(idx) = Self::SELECTORS.binary_search(&selector) else {
                return Err(
                    alloy_sol_types::Error::unknown_selector(
                        <Self as alloy_sol_types::SolInterface>::NAME,
                        selector,
                    ),
                );
            };
            DECODE_VALIDATE_SHIMS[idx](data)
        }
        #[inline]
        fn abi_encoded_size(&self) -> usize {
            match self {
                Self::AccessControlBadConfirmation(inner) => {
                    <AccessControlBadConfirmation as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::AccessControlUnauthorizedAccount(inner) => {
                    <AccessControlUnauthorizedAccount as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::AmountMismatch(inner) => {
                    <AmountMismatch as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::DeploymentFailed(inner) => {
                    <DeploymentFailed as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::FailedDeployment(inner) => {
                    <FailedDeployment as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InsufficientBalance(inner) => {
                    <InsufficientBalance as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidLiFiDestinationChain(inner) => {
                    <InvalidLiFiDestinationChain as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidLiFiReceiver(inner) => {
                    <InvalidLiFiReceiver as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::OwnableInvalidOwner(inner) => {
                    <OwnableInvalidOwner as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::OwnableUnauthorizedAccount(inner) => {
                    <OwnableUnauthorizedAccount as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::UnsupportedBridging(inner) => {
                    <UnsupportedBridging as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::UnsupportedShielding(inner) => {
                    <UnsupportedShielding as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
            }
        }
        #[inline]
        fn abi_encode_raw(&self, out: &mut alloy_sol_types::private::Vec<u8>) {
            match self {
                Self::AccessControlBadConfirmation(inner) => {
                    <AccessControlBadConfirmation as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::AccessControlUnauthorizedAccount(inner) => {
                    <AccessControlUnauthorizedAccount as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::AmountMismatch(inner) => {
                    <AmountMismatch as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::DeploymentFailed(inner) => {
                    <DeploymentFailed as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::FailedDeployment(inner) => {
                    <FailedDeployment as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InsufficientBalance(inner) => {
                    <InsufficientBalance as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidLiFiDestinationChain(inner) => {
                    <InvalidLiFiDestinationChain as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidLiFiReceiver(inner) => {
                    <InvalidLiFiReceiver as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::OwnableInvalidOwner(inner) => {
                    <OwnableInvalidOwner as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::OwnableUnauthorizedAccount(inner) => {
                    <OwnableUnauthorizedAccount as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::UnsupportedBridging(inner) => {
                    <UnsupportedBridging as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::UnsupportedShielding(inner) => {
                    <UnsupportedShielding as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
            }
        }
    }
    ///Container for all the [`PortalFactory`](self) events.
    #[derive(Clone)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub enum PortalFactoryEvents {
        #[allow(missing_docs)]
        ConfigUpdated(ConfigUpdated),
        #[allow(missing_docs)]
        EntryBridgePortalDeployed(EntryBridgePortalDeployed),
        #[allow(missing_docs)]
        ExitBridgePortalDeployed(ExitBridgePortalDeployed),
        #[allow(missing_docs)]
        OwnershipTransferred(OwnershipTransferred),
        #[allow(missing_docs)]
        RecoveryPortalDeployed(RecoveryPortalDeployed),
        #[allow(missing_docs)]
        RoleAdminChanged(RoleAdminChanged),
        #[allow(missing_docs)]
        RoleGranted(RoleGranted),
        #[allow(missing_docs)]
        RoleRevoked(RoleRevoked),
        #[allow(missing_docs)]
        ShieldPortalDeployed(ShieldPortalDeployed),
        #[allow(missing_docs)]
        SolanaExitBridgePortalDeployed(SolanaExitBridgePortalDeployed),
        #[allow(missing_docs)]
        SolanaRecoveryPortalDeployed(SolanaRecoveryPortalDeployed),
    }
    impl PortalFactoryEvents {
        /// All the selectors of this enum.
        ///
        /// Note that the selectors might not be in the same order as the variants.
        /// No guarantees are made about the order of the selectors.
        ///
        /// Prefer using `SolInterface` methods instead.
        pub const SELECTORS: &'static [[u8; 32usize]] = &[
            [
                13u8, 88u8, 12u8, 16u8, 150u8, 11u8, 217u8, 179u8, 9u8, 8u8, 218u8, 78u8,
                223u8, 142u8, 95u8, 91u8, 166u8, 177u8, 251u8, 175u8, 44u8, 50u8, 210u8,
                139u8, 225u8, 117u8, 169u8, 45u8, 75u8, 152u8, 150u8, 242u8,
            ],
            [
                33u8, 19u8, 133u8, 159u8, 91u8, 105u8, 131u8, 167u8, 7u8, 38u8, 147u8,
                42u8, 179u8, 81u8, 142u8, 91u8, 244u8, 84u8, 247u8, 49u8, 253u8, 177u8,
                109u8, 150u8, 216u8, 172u8, 135u8, 10u8, 91u8, 35u8, 34u8, 65u8,
            ],
            [
                33u8, 89u8, 169u8, 190u8, 172u8, 58u8, 107u8, 223u8, 72u8, 139u8, 229u8,
                138u8, 64u8, 203u8, 77u8, 243u8, 199u8, 100u8, 144u8, 130u8, 147u8, 58u8,
                41u8, 15u8, 29u8, 134u8, 164u8, 62u8, 187u8, 162u8, 210u8, 19u8,
            ],
            [
                36u8, 138u8, 208u8, 182u8, 23u8, 53u8, 70u8, 245u8, 182u8, 140u8, 161u8,
                234u8, 132u8, 147u8, 135u8, 31u8, 178u8, 32u8, 86u8, 27u8, 87u8, 125u8,
                125u8, 95u8, 223u8, 123u8, 244u8, 4u8, 193u8, 193u8, 248u8, 121u8,
            ],
            [
                47u8, 135u8, 136u8, 17u8, 126u8, 126u8, 255u8, 29u8, 130u8, 233u8, 38u8,
                236u8, 121u8, 73u8, 1u8, 209u8, 124u8, 120u8, 2u8, 74u8, 80u8, 39u8, 9u8,
                64u8, 48u8, 69u8, 64u8, 167u8, 51u8, 101u8, 111u8, 13u8,
            ],
            [
                113u8, 83u8, 252u8, 27u8, 161u8, 216u8, 166u8, 120u8, 78u8, 10u8, 58u8,
                161u8, 17u8, 62u8, 49u8, 158u8, 191u8, 103u8, 2u8, 204u8, 189u8, 204u8,
                131u8, 161u8, 38u8, 165u8, 210u8, 59u8, 38u8, 113u8, 176u8, 116u8,
            ],
            [
                139u8, 224u8, 7u8, 156u8, 83u8, 22u8, 89u8, 20u8, 19u8, 68u8, 205u8,
                31u8, 208u8, 164u8, 242u8, 132u8, 25u8, 73u8, 127u8, 151u8, 34u8, 163u8,
                218u8, 175u8, 227u8, 180u8, 24u8, 111u8, 107u8, 100u8, 87u8, 224u8,
            ],
            [
                189u8, 121u8, 184u8, 111u8, 254u8, 10u8, 184u8, 232u8, 119u8, 97u8, 81u8,
                81u8, 66u8, 23u8, 205u8, 124u8, 172u8, 213u8, 44u8, 144u8, 159u8, 102u8,
                71u8, 92u8, 58u8, 244u8, 78u8, 18u8, 159u8, 11u8, 0u8, 255u8,
            ],
            [
                211u8, 46u8, 252u8, 239u8, 161u8, 25u8, 151u8, 235u8, 234u8, 19u8, 228u8,
                195u8, 101u8, 162u8, 169u8, 46u8, 239u8, 199u8, 185u8, 101u8, 167u8,
                75u8, 161u8, 81u8, 218u8, 219u8, 2u8, 163u8, 205u8, 80u8, 6u8, 55u8,
            ],
            [
                246u8, 57u8, 31u8, 92u8, 50u8, 217u8, 198u8, 157u8, 42u8, 71u8, 234u8,
                103u8, 11u8, 68u8, 41u8, 116u8, 181u8, 57u8, 53u8, 209u8, 237u8, 199u8,
                253u8, 100u8, 235u8, 33u8, 224u8, 71u8, 168u8, 57u8, 23u8, 27u8,
            ],
            [
                255u8, 138u8, 151u8, 253u8, 167u8, 114u8, 132u8, 149u8, 238u8, 58u8,
                92u8, 85u8, 26u8, 243u8, 73u8, 94u8, 75u8, 162u8, 60u8, 221u8, 162u8,
                172u8, 19u8, 132u8, 145u8, 239u8, 242u8, 148u8, 144u8, 33u8, 34u8, 238u8,
            ],
        ];
        /// The names of the variants in the same order as `SELECTORS`.
        pub const VARIANT_NAMES: &'static [&'static str] = &[
            ::core::stringify!(EntryBridgePortalDeployed),
            ::core::stringify!(SolanaRecoveryPortalDeployed),
            ::core::stringify!(SolanaExitBridgePortalDeployed),
            ::core::stringify!(RecoveryPortalDeployed),
            ::core::stringify!(RoleGranted),
            ::core::stringify!(ShieldPortalDeployed),
            ::core::stringify!(OwnershipTransferred),
            ::core::stringify!(RoleAdminChanged),
            ::core::stringify!(ExitBridgePortalDeployed),
            ::core::stringify!(RoleRevoked),
            ::core::stringify!(ConfigUpdated),
        ];
        /// The signatures in the same order as `SELECTORS`.
        pub const SIGNATURES: &'static [&'static str] = &[
            <EntryBridgePortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE,
            <SolanaRecoveryPortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE,
            <SolanaExitBridgePortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE,
            <RecoveryPortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE,
            <RoleGranted as alloy_sol_types::SolEvent>::SIGNATURE,
            <ShieldPortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE,
            <OwnershipTransferred as alloy_sol_types::SolEvent>::SIGNATURE,
            <RoleAdminChanged as alloy_sol_types::SolEvent>::SIGNATURE,
            <ExitBridgePortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE,
            <RoleRevoked as alloy_sol_types::SolEvent>::SIGNATURE,
            <ConfigUpdated as alloy_sol_types::SolEvent>::SIGNATURE,
        ];
        /// Returns the signature for the given selector, if known.
        #[inline]
        pub fn signature_by_selector(
            selector: [u8; 32usize],
        ) -> ::core::option::Option<&'static str> {
            match Self::SELECTORS.binary_search(&selector) {
                ::core::result::Result::Ok(idx) => {
                    ::core::option::Option::Some(Self::SIGNATURES[idx])
                }
                ::core::result::Result::Err(_) => ::core::option::Option::None,
            }
        }
        /// Returns the enum variant name for the given selector, if known.
        #[inline]
        pub fn name_by_selector(
            selector: [u8; 32usize],
        ) -> ::core::option::Option<&'static str> {
            let sig = Self::signature_by_selector(selector)?;
            sig.split_once('(').map(|(name, _)| name)
        }
    }
    #[automatically_derived]
    impl alloy_sol_types::SolEventInterface for PortalFactoryEvents {
        const NAME: &'static str = "PortalFactoryEvents";
        const COUNT: usize = 11usize;
        fn decode_raw_log(
            topics: &[alloy_sol_types::Word],
            data: &[u8],
        ) -> alloy_sol_types::Result<Self> {
            match topics.first().copied() {
                Some(<ConfigUpdated as alloy_sol_types::SolEvent>::SIGNATURE_HASH) => {
                    <ConfigUpdated as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::ConfigUpdated)
                }
                Some(
                    <EntryBridgePortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <EntryBridgePortalDeployed as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::EntryBridgePortalDeployed)
                }
                Some(
                    <ExitBridgePortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <ExitBridgePortalDeployed as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::ExitBridgePortalDeployed)
                }
                Some(
                    <OwnershipTransferred as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <OwnershipTransferred as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::OwnershipTransferred)
                }
                Some(
                    <RecoveryPortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <RecoveryPortalDeployed as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::RecoveryPortalDeployed)
                }
                Some(<RoleAdminChanged as alloy_sol_types::SolEvent>::SIGNATURE_HASH) => {
                    <RoleAdminChanged as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::RoleAdminChanged)
                }
                Some(<RoleGranted as alloy_sol_types::SolEvent>::SIGNATURE_HASH) => {
                    <RoleGranted as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::RoleGranted)
                }
                Some(<RoleRevoked as alloy_sol_types::SolEvent>::SIGNATURE_HASH) => {
                    <RoleRevoked as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::RoleRevoked)
                }
                Some(
                    <ShieldPortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <ShieldPortalDeployed as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::ShieldPortalDeployed)
                }
                Some(
                    <SolanaExitBridgePortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <SolanaExitBridgePortalDeployed as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::SolanaExitBridgePortalDeployed)
                }
                Some(
                    <SolanaRecoveryPortalDeployed as alloy_sol_types::SolEvent>::SIGNATURE_HASH,
                ) => {
                    <SolanaRecoveryPortalDeployed as alloy_sol_types::SolEvent>::decode_raw_log(
                            topics,
                            data,
                        )
                        .map(Self::SolanaRecoveryPortalDeployed)
                }
                _ => {
                    alloy_sol_types::private::Err(alloy_sol_types::Error::InvalidLog {
                        name: <Self as alloy_sol_types::SolEventInterface>::NAME,
                        log: alloy_sol_types::private::Box::new(
                            alloy_sol_types::private::LogData::new_unchecked(
                                topics.to_vec(),
                                data.to_vec().into(),
                            ),
                        ),
                    })
                }
            }
        }
    }
    #[automatically_derived]
    impl alloy_sol_types::private::IntoLogData for PortalFactoryEvents {
        fn to_log_data(&self) -> alloy_sol_types::private::LogData {
            match self {
                Self::ConfigUpdated(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::EntryBridgePortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::ExitBridgePortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::OwnershipTransferred(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::RecoveryPortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::RoleAdminChanged(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::RoleGranted(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::RoleRevoked(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::ShieldPortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::SolanaExitBridgePortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
                Self::SolanaRecoveryPortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::to_log_data(inner)
                }
            }
        }
        fn into_log_data(self) -> alloy_sol_types::private::LogData {
            match self {
                Self::ConfigUpdated(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::EntryBridgePortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::ExitBridgePortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::OwnershipTransferred(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::RecoveryPortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::RoleAdminChanged(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::RoleGranted(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::RoleRevoked(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::ShieldPortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::SolanaExitBridgePortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
                Self::SolanaRecoveryPortalDeployed(inner) => {
                    alloy_sol_types::private::IntoLogData::into_log_data(inner)
                }
            }
        }
    }
    use alloy::contract as alloy_contract;
    /**Creates a new wrapper around an on-chain [`PortalFactory`](self) contract instance.

See the [wrapper's documentation](`PortalFactoryInstance`) for more details.*/
    #[inline]
    pub const fn new<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    >(
        address: alloy_sol_types::private::Address,
        __provider: P,
    ) -> PortalFactoryInstance<P, N> {
        PortalFactoryInstance::<P, N>::new(address, __provider)
    }
    /**Deploys this contract using the given `provider` and constructor arguments, if any.

Returns a new instance of the contract, if the deployment was successful.

For more fine-grained control over the deployment process, use [`deploy_builder`] instead.*/
    #[inline]
    pub fn deploy<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    >(
        __provider: P,
        initialOwner: alloy::sol_types::private::Address,
    ) -> impl ::core::future::Future<
        Output = alloy_contract::Result<PortalFactoryInstance<P, N>>,
    > {
        PortalFactoryInstance::<P, N>::deploy(__provider, initialOwner)
    }
    /**Creates a `RawCallBuilder` for deploying this contract using the given `provider`
and constructor arguments, if any.

This is a simple wrapper around creating a `RawCallBuilder` with the data set to
the bytecode concatenated with the constructor's ABI-encoded arguments.*/
    #[inline]
    pub fn deploy_builder<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    >(
        __provider: P,
        initialOwner: alloy::sol_types::private::Address,
    ) -> alloy_contract::RawCallBuilder<P, N> {
        PortalFactoryInstance::<P, N>::deploy_builder(__provider, initialOwner)
    }
    /**A [`PortalFactory`](self) instance.

Contains type-safe methods for interacting with an on-chain instance of the
[`PortalFactory`](self) contract located at a given `address`, using a given
provider `P`.

If the contract bytecode is available (see the [`sol!`](alloy_sol_types::sol!)
documentation on how to provide it), the `deploy` and `deploy_builder` methods can
be used to deploy a new instance of the contract.

See the [module-level documentation](self) for all the available methods.*/
    #[derive(Clone)]
    pub struct PortalFactoryInstance<P, N = alloy_contract::private::Ethereum> {
        address: alloy_sol_types::private::Address,
        provider: P,
        _network: ::core::marker::PhantomData<N>,
    }
    #[automatically_derived]
    impl<P, N> ::core::fmt::Debug for PortalFactoryInstance<P, N> {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple("PortalFactoryInstance").field(&self.address).finish()
        }
    }
    /// Instantiation and getters/setters.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > PortalFactoryInstance<P, N> {
        /**Creates a new wrapper around an on-chain [`PortalFactory`](self) contract instance.

See the [wrapper's documentation](`PortalFactoryInstance`) for more details.*/
        #[inline]
        pub const fn new(
            address: alloy_sol_types::private::Address,
            __provider: P,
        ) -> Self {
            Self {
                address,
                provider: __provider,
                _network: ::core::marker::PhantomData,
            }
        }
        /**Deploys this contract using the given `provider` and constructor arguments, if any.

Returns a new instance of the contract, if the deployment was successful.

For more fine-grained control over the deployment process, use [`deploy_builder`] instead.*/
        #[inline]
        pub async fn deploy(
            __provider: P,
            initialOwner: alloy::sol_types::private::Address,
        ) -> alloy_contract::Result<PortalFactoryInstance<P, N>> {
            let call_builder = Self::deploy_builder(__provider, initialOwner);
            let contract_address = call_builder.deploy().await?;
            Ok(Self::new(contract_address, call_builder.provider))
        }
        /**Creates a `RawCallBuilder` for deploying this contract using the given `provider`
and constructor arguments, if any.

This is a simple wrapper around creating a `RawCallBuilder` with the data set to
the bytecode concatenated with the constructor's ABI-encoded arguments.*/
        #[inline]
        pub fn deploy_builder(
            __provider: P,
            initialOwner: alloy::sol_types::private::Address,
        ) -> alloy_contract::RawCallBuilder<P, N> {
            alloy_contract::RawCallBuilder::new_raw_deploy(
                __provider,
                [
                    &BYTECODE[..],
                    &alloy_sol_types::SolConstructor::abi_encode(
                        &constructorCall { initialOwner },
                    )[..],
                ]
                    .concat()
                    .into(),
            )
        }
        /// Returns a reference to the address.
        #[inline]
        pub const fn address(&self) -> &alloy_sol_types::private::Address {
            &self.address
        }
        /// Sets the address.
        #[inline]
        pub fn set_address(&mut self, address: alloy_sol_types::private::Address) {
            self.address = address;
        }
        /// Sets the address and returns `self`.
        pub fn at(mut self, address: alloy_sol_types::private::Address) -> Self {
            self.set_address(address);
            self
        }
        /// Returns a reference to the provider.
        #[inline]
        pub const fn provider(&self) -> &P {
            &self.provider
        }
    }
    impl<P: ::core::clone::Clone, N> PortalFactoryInstance<&P, N> {
        /// Clones the provider and returns a new instance with the cloned provider.
        #[inline]
        pub fn with_cloned_provider(self) -> PortalFactoryInstance<P, N> {
            PortalFactoryInstance {
                address: self.address,
                provider: ::core::clone::Clone::clone(&self.provider),
                _network: ::core::marker::PhantomData,
            }
        }
    }
    /// Function calls.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > PortalFactoryInstance<P, N> {
        /// Creates a new call builder using this contract instance's provider and address.
        ///
        /// Note that the call can be any function call, not just those defined in this
        /// contract. Prefer using the other methods for building type-safe contract calls.
        pub fn call_builder<C: alloy_sol_types::SolCall>(
            &self,
            call: &C,
        ) -> alloy_contract::SolCallBuilder<&P, C, N> {
            alloy_contract::SolCallBuilder::new_sol(&self.provider, &self.address, call)
        }
        ///Creates a new call builder for the [`AUTHORITY_ROLE`] function.
        pub fn AUTHORITY_ROLE(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, AUTHORITY_ROLECall, N> {
            self.call_builder(&AUTHORITY_ROLECall)
        }
        ///Creates a new call builder for the [`DEFAULT_ADMIN_ROLE`] function.
        pub fn DEFAULT_ADMIN_ROLE(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, DEFAULT_ADMIN_ROLECall, N> {
            self.call_builder(&DEFAULT_ADMIN_ROLECall)
        }
        ///Creates a new call builder for the [`OPERATOR_ROLE`] function.
        pub fn OPERATOR_ROLE(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, OPERATOR_ROLECall, N> {
            self.call_builder(&OPERATOR_ROLECall)
        }
        ///Creates a new call builder for the [`deployEntryBridgePortal`] function.
        pub fn deployEntryBridgePortal(
            &self,
            bridgeData: alloy::sol_types::private::Bytes,
            note: <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
            currency: alloy::sol_types::private::Address,
            recovery: alloy::sol_types::private::Address,
            gasFee: alloy::sol_types::private::primitives::aliases::U256,
        ) -> alloy_contract::SolCallBuilder<&P, deployEntryBridgePortalCall, N> {
            self.call_builder(
                &deployEntryBridgePortalCall {
                    bridgeData,
                    note,
                    currency,
                    recovery,
                    gasFee,
                },
            )
        }
        ///Creates a new call builder for the [`deployExitBridgePortal`] function.
        pub fn deployExitBridgePortal(
            &self,
            bridgeData: alloy::sol_types::private::Bytes,
            amount: alloy::sol_types::private::primitives::aliases::U256,
            currency: alloy::sol_types::private::Address,
            exitAddress: alloy::sol_types::private::Address,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
            gasFee: alloy::sol_types::private::primitives::aliases::U256,
        ) -> alloy_contract::SolCallBuilder<&P, deployExitBridgePortalCall, N> {
            self.call_builder(
                &deployExitBridgePortalCall {
                    bridgeData,
                    amount,
                    currency,
                    exitAddress,
                    exitChainId,
                    recovery,
                    gasFee,
                },
            )
        }
        ///Creates a new call builder for the [`deployRecoveryEntryPortal`] function.
        pub fn deployRecoveryEntryPortal(
            &self,
            ownerHash: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
            tokenAddress: alloy::sol_types::private::Address,
            to: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, deployRecoveryEntryPortalCall, N> {
            self.call_builder(
                &deployRecoveryEntryPortalCall {
                    ownerHash,
                    recovery,
                    tokenAddress,
                    to,
                },
            )
        }
        ///Creates a new call builder for the [`deployRecoveryExitPortal`] function.
        pub fn deployRecoveryExitPortal(
            &self,
            exitAddress: alloy::sol_types::private::Address,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
            tokenAddress: alloy::sol_types::private::Address,
            to: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, deployRecoveryExitPortalCall, N> {
            self.call_builder(
                &deployRecoveryExitPortalCall {
                    exitAddress,
                    exitChainId,
                    recovery,
                    tokenAddress,
                    to,
                },
            )
        }
        ///Creates a new call builder for the [`deployShieldPortal`] function.
        pub fn deployShieldPortal(
            &self,
            note: <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
            recovery: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, deployShieldPortalCall, N> {
            self.call_builder(
                &deployShieldPortalCall {
                    note,
                    recovery,
                },
            )
        }
        ///Creates a new call builder for the [`deploySolanaExitBridgePortal`] function.
        pub fn deploySolanaExitBridgePortal(
            &self,
            bridgeData: alloy::sol_types::private::Bytes,
            amount: alloy::sol_types::private::primitives::aliases::U256,
            currency: alloy::sol_types::private::Address,
            exitAddress: alloy::sol_types::private::FixedBytes<32>,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
            gasFee: alloy::sol_types::private::primitives::aliases::U256,
        ) -> alloy_contract::SolCallBuilder<&P, deploySolanaExitBridgePortalCall, N> {
            self.call_builder(
                &deploySolanaExitBridgePortalCall {
                    bridgeData,
                    amount,
                    currency,
                    exitAddress,
                    exitChainId,
                    recovery,
                    gasFee,
                },
            )
        }
        ///Creates a new call builder for the [`deploySolanaRecoveryExitPortal`] function.
        pub fn deploySolanaRecoveryExitPortal(
            &self,
            exitAddress: alloy::sol_types::private::FixedBytes<32>,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
            tokenAddress: alloy::sol_types::private::Address,
            to: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, deploySolanaRecoveryExitPortalCall, N> {
            self.call_builder(
                &deploySolanaRecoveryExitPortalCall {
                    exitAddress,
                    exitChainId,
                    recovery,
                    tokenAddress,
                    to,
                },
            )
        }
        ///Creates a new call builder for the [`getEntryPortalAddress`] function.
        pub fn getEntryPortalAddress(
            &self,
            ownerHash: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, getEntryPortalAddressCall, N> {
            self.call_builder(
                &getEntryPortalAddressCall {
                    ownerHash,
                    recovery,
                },
            )
        }
        ///Creates a new call builder for the [`getExitPortalAddress`] function.
        pub fn getExitPortalAddress(
            &self,
            exitAddress: alloy::sol_types::private::Address,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, getExitPortalAddressCall, N> {
            self.call_builder(
                &getExitPortalAddressCall {
                    exitAddress,
                    exitChainId,
                    recovery,
                },
            )
        }
        ///Creates a new call builder for the [`getRoleAdmin`] function.
        pub fn getRoleAdmin(
            &self,
            role: alloy::sol_types::private::FixedBytes<32>,
        ) -> alloy_contract::SolCallBuilder<&P, getRoleAdminCall, N> {
            self.call_builder(&getRoleAdminCall { role })
        }
        ///Creates a new call builder for the [`getSolanaExitPortalAddress`] function.
        pub fn getSolanaExitPortalAddress(
            &self,
            exitAddress: alloy::sol_types::private::FixedBytes<32>,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            recovery: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, getSolanaExitPortalAddressCall, N> {
            self.call_builder(
                &getSolanaExitPortalAddressCall {
                    exitAddress,
                    exitChainId,
                    recovery,
                },
            )
        }
        ///Creates a new call builder for the [`grantRole`] function.
        pub fn grantRole(
            &self,
            role: alloy::sol_types::private::FixedBytes<32>,
            account: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, grantRoleCall, N> {
            self.call_builder(&grantRoleCall { role, account })
        }
        ///Creates a new call builder for the [`hasRole`] function.
        pub fn hasRole(
            &self,
            role: alloy::sol_types::private::FixedBytes<32>,
            account: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, hasRoleCall, N> {
            self.call_builder(&hasRoleCall { role, account })
        }
        ///Creates a new call builder for the [`owner`] function.
        pub fn owner(&self) -> alloy_contract::SolCallBuilder<&P, ownerCall, N> {
            self.call_builder(&ownerCall)
        }
        ///Creates a new call builder for the [`portalImpl`] function.
        pub fn portalImpl(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, portalImplCall, N> {
            self.call_builder(&portalImplCall)
        }
        ///Creates a new call builder for the [`portalIsRegistered`] function.
        pub fn portalIsRegistered(
            &self,
            portalAddress: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, portalIsRegisteredCall, N> {
            self.call_builder(
                &portalIsRegisteredCall {
                    portalAddress,
                },
            )
        }
        ///Creates a new call builder for the [`renounceOwnership`] function.
        pub fn renounceOwnership(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, renounceOwnershipCall, N> {
            self.call_builder(&renounceOwnershipCall)
        }
        ///Creates a new call builder for the [`renounceRole`] function.
        pub fn renounceRole(
            &self,
            role: alloy::sol_types::private::FixedBytes<32>,
            callerConfirmation: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, renounceRoleCall, N> {
            self.call_builder(
                &renounceRoleCall {
                    role,
                    callerConfirmation,
                },
            )
        }
        ///Creates a new call builder for the [`revokeRole`] function.
        pub fn revokeRole(
            &self,
            role: alloy::sol_types::private::FixedBytes<32>,
            account: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, revokeRoleCall, N> {
            self.call_builder(&revokeRoleCall { role, account })
        }
        ///Creates a new call builder for the [`solanaPortalImpl`] function.
        pub fn solanaPortalImpl(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, solanaPortalImplCall, N> {
            self.call_builder(&solanaPortalImplCall)
        }
        ///Creates a new call builder for the [`supportsInterface`] function.
        pub fn supportsInterface(
            &self,
            interfaceId: alloy::sol_types::private::FixedBytes<4>,
        ) -> alloy_contract::SolCallBuilder<&P, supportsInterfaceCall, N> {
            self.call_builder(
                &supportsInterfaceCall {
                    interfaceId,
                },
            )
        }
        ///Creates a new call builder for the [`transferOwnership`] function.
        pub fn transferOwnership(
            &self,
            newOwner: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, transferOwnershipCall, N> {
            self.call_builder(&transferOwnershipCall { newOwner })
        }
        ///Creates a new call builder for the [`updateConfig`] function.
        pub fn updateConfig(
            &self,
            curvyVaultProxyAddress: alloy::sol_types::private::Address,
            curvyAggregatorAlphaProxyAddress: alloy::sol_types::private::Address,
            lifiDiamondAddress: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, updateConfigCall, N> {
            self.call_builder(
                &updateConfigCall {
                    curvyVaultProxyAddress,
                    curvyAggregatorAlphaProxyAddress,
                    lifiDiamondAddress,
                },
            )
        }
    }
    /// Event filters.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > PortalFactoryInstance<P, N> {
        /// Creates a new event filter using this contract instance's provider and address.
        ///
        /// Note that the type can be any event, not just those defined in this contract.
        /// Prefer using the other methods for building type-safe event filters.
        pub fn event_filter<E: alloy_sol_types::SolEvent>(
            &self,
        ) -> alloy_contract::Event<&P, E, N> {
            alloy_contract::Event::new_sol(&self.provider, &self.address)
        }
        ///Creates a new event filter for the [`ConfigUpdated`] event.
        pub fn ConfigUpdated_filter(
            &self,
        ) -> alloy_contract::Event<&P, ConfigUpdated, N> {
            self.event_filter::<ConfigUpdated>()
        }
        ///Creates a new event filter for the [`EntryBridgePortalDeployed`] event.
        pub fn EntryBridgePortalDeployed_filter(
            &self,
        ) -> alloy_contract::Event<&P, EntryBridgePortalDeployed, N> {
            self.event_filter::<EntryBridgePortalDeployed>()
        }
        ///Creates a new event filter for the [`ExitBridgePortalDeployed`] event.
        pub fn ExitBridgePortalDeployed_filter(
            &self,
        ) -> alloy_contract::Event<&P, ExitBridgePortalDeployed, N> {
            self.event_filter::<ExitBridgePortalDeployed>()
        }
        ///Creates a new event filter for the [`OwnershipTransferred`] event.
        pub fn OwnershipTransferred_filter(
            &self,
        ) -> alloy_contract::Event<&P, OwnershipTransferred, N> {
            self.event_filter::<OwnershipTransferred>()
        }
        ///Creates a new event filter for the [`RecoveryPortalDeployed`] event.
        pub fn RecoveryPortalDeployed_filter(
            &self,
        ) -> alloy_contract::Event<&P, RecoveryPortalDeployed, N> {
            self.event_filter::<RecoveryPortalDeployed>()
        }
        ///Creates a new event filter for the [`RoleAdminChanged`] event.
        pub fn RoleAdminChanged_filter(
            &self,
        ) -> alloy_contract::Event<&P, RoleAdminChanged, N> {
            self.event_filter::<RoleAdminChanged>()
        }
        ///Creates a new event filter for the [`RoleGranted`] event.
        pub fn RoleGranted_filter(&self) -> alloy_contract::Event<&P, RoleGranted, N> {
            self.event_filter::<RoleGranted>()
        }
        ///Creates a new event filter for the [`RoleRevoked`] event.
        pub fn RoleRevoked_filter(&self) -> alloy_contract::Event<&P, RoleRevoked, N> {
            self.event_filter::<RoleRevoked>()
        }
        ///Creates a new event filter for the [`ShieldPortalDeployed`] event.
        pub fn ShieldPortalDeployed_filter(
            &self,
        ) -> alloy_contract::Event<&P, ShieldPortalDeployed, N> {
            self.event_filter::<ShieldPortalDeployed>()
        }
        ///Creates a new event filter for the [`SolanaExitBridgePortalDeployed`] event.
        pub fn SolanaExitBridgePortalDeployed_filter(
            &self,
        ) -> alloy_contract::Event<&P, SolanaExitBridgePortalDeployed, N> {
            self.event_filter::<SolanaExitBridgePortalDeployed>()
        }
        ///Creates a new event filter for the [`SolanaRecoveryPortalDeployed`] event.
        pub fn SolanaRecoveryPortalDeployed_filter(
            &self,
        ) -> alloy_contract::Event<&P, SolanaRecoveryPortalDeployed, N> {
            self.event_filter::<SolanaRecoveryPortalDeployed>()
        }
    }
}
