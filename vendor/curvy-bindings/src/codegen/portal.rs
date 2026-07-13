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

interface Portal {
    error AlreadyInitialized();
    error BridgeCallFailed();
    error InsufficientBalanceForLiFiBridging();
    error InvalidLiFiAddress();
    error InvalidOwnerHash();
    error InvalidOwnerHashOrExitBridgeData();
    error InvalidRecoveryAddress();
    error InvalidSignatureOrTamperedData();
    error NoBalance();
    error SafeERC20FailedOperation(address token);

    constructor();

    function bridge(address lifiDiamondAddress, bytes memory bridgeData, uint256 amount, address currency, uint256 gasFee) external payable;
    function curvyAggregator() external view returns (address);
    function curvyVault() external view returns (address);
    function initialize(uint256 ownerHash, address exitAddress, uint256 exitChainId, address _recovery) external;
    function recover(address tokenAddress, address to) external;
    function recovery() external view returns (address);
    function shield(CurvyTypes.Note memory note, address curvyAggregatorAlphaProxyAddress, address curvyVaultProxyAddress) external;
}
```

...which was generated by the following JSON ABI:
```json
[
  {
    "type": "constructor",
    "inputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "bridge",
    "inputs": [
      {
        "name": "lifiDiamondAddress",
        "type": "address",
        "internalType": "address"
      },
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
    "name": "curvyAggregator",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "contract ICurvyAggregatorAlpha"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "curvyVault",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "contract ICurvyVault"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "initialize",
    "inputs": [
      {
        "name": "ownerHash",
        "type": "uint256",
        "internalType": "uint256"
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
        "name": "_recovery",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "recover",
    "inputs": [
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
    "name": "recovery",
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
    "name": "shield",
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
        "name": "curvyAggregatorAlphaProxyAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "curvyVaultProxyAddress",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "error",
    "name": "AlreadyInitialized",
    "inputs": []
  },
  {
    "type": "error",
    "name": "BridgeCallFailed",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InsufficientBalanceForLiFiBridging",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InvalidLiFiAddress",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InvalidOwnerHash",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InvalidOwnerHashOrExitBridgeData",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InvalidRecoveryAddress",
    "inputs": []
  },
  {
    "type": "error",
    "name": "InvalidSignatureOrTamperedData",
    "inputs": []
  },
  {
    "type": "error",
    "name": "NoBalance",
    "inputs": []
  },
  {
    "type": "error",
    "name": "SafeERC20FailedOperation",
    "inputs": [
      {
        "name": "token",
        "type": "address",
        "internalType": "address"
      }
    ]
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
pub mod Portal {
    use super::*;
    use alloy::sol_types as alloy_sol_types;
    /// The creation / init bytecode of the contract.
    ///
    /// ```text
    ///0x6080604052348015600e575f5ffd5b506005805460ff60a81b1916600160a81b179055610f058061002f5f395ff3fe60806040526004361061006e575f3560e01c8063648bf7741161004c578063648bf774146100ed57806376fceb761461010c578063ddceafa91461012b578063e3f5c5521461014a575f5ffd5b8063199c544e146100725780633fb07027146100935780635dc24ee3146100ce575b5f5ffd5b34801561007d575f5ffd5b5061009161008c366004610c21565b61015d565b005b34801561009e575f5ffd5b506004546100b2906001600160a01b031681565b6040516001600160a01b03909116815260200160405180910390f35b3480156100d9575f5ffd5b506003546100b2906001600160a01b031681565b3480156100f8575f5ffd5b50610091610107366004610ce6565b6103a2565b348015610117575f5ffd5b50610091610126366004610d1d565b6105b9565b348015610136575f5ffd5b506005546100b2906001600160a01b031681565b610091610158366004610d64565b61068e565b600554600160a01b900460ff16156101b65760405162461bcd60e51b815260206004820152601760248201527614da5b99db19555cd94e88105b1c9958591e481d5cd959604a1b60448201526064015b60405180910390fd5b5f548351146101d75760405162cb7dff60e81b815260040160405180910390fd5b600380546001600160a01b038481166001600160a01b0319928316179092556004805492841692909116821781556020850151604051630cf99be760e31b8152918201525f91906367ccdf3890602401602060405180830381865afa158015610242573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906102669190610e08565b90506001600160a01b0381161580159061029d57506001600160a01b03811673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b156103245760035460408501516102c2916001600160a01b03848116929116906109df565b60035460405163ba48d11760e01b81526001600160a01b039091169063ba48d117906102f2908790600401610e2a565b5f604051808303815f87803b158015610309575f5ffd5b505af115801561031b573d5f5f3e3d5ffd5b50505050610389565b600354604080860151905163ba48d11760e01b81526001600160a01b039092169163ba48d117919061035a908890600401610e2a565b5f604051808303818588803b158015610371575f5ffd5b505af1158015610383573d5f5f3e3d5ffd5b50505050505b50506005805460ff60a01b1916600160a01b1790555050565b6005546001600160a01b031632146103f45760405162461bcd60e51b8152602060048201526015602482015274506f7274616c3a204f6e6c79207265636f7665727960581b60448201526064016101ad565b6001600160a01b03811661041b5760405163530a10d160e11b815260040160405180910390fd5b6001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee148061044d57506001600160a01b038216155b1561051957475f81900361047457604051636165515360e11b815260040160405180910390fd5b5f826001600160a01b0316826040515f6040518083038185875af1925050503d805f81146104bd576040519150601f19603f3d011682016040523d82523d5f602084013e6104c2565b606091505b50509050806105135760405162461bcd60e51b815260206004820152601b60248201527f506f7274616c3a20455448207472616e73666572206661696c6564000000000060448201526064016101ad565b50505050565b6040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa15801561055f573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906105839190610e8a565b9050805f036105a557604051636165515360e11b815260040160405180910390fd5b6105136001600160a01b0383168483610a9c565b600554600160a81b900460ff16156105e35760405162dc149f60e41b815260040160405180910390fd5b6005805460ff60a81b1916600160a81b1790556001600160a01b03811661061d5760405163530a10d160e11b815260040160405180910390fd5b83156001600160a01b038416151480610637575083158215145b1561065557604051631d4deb8b60e31b815260040160405180910390fd5b5f93909355600180546001600160a01b039384166001600160a01b03199182161790915560029190915560058054929093169116179055565b600554600160a01b900460ff16156106e25760405162461bcd60e51b815260206004820152601760248201527614da5b99db19555cd94e88105b1c9958591e481d5cd959604a1b60448201526064016101ad565b6001600160a01b0382161580159061071757506001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b15610883576040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa158015610762573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906107869190610e8a565b9050848110156107a957604051637bdd7ae760e01b815260040160405180910390fd5b6107bd6001600160a01b03831689876109df565b5f5f896001600160a01b0316348a8a6040516107da929190610ea1565b5f6040518083038185875af1925050503d805f8114610814576040519150601f19603f3d011682016040523d82523d5f602084013e610819565b606091505b5090925090506108336001600160a01b0385168b5f6109df565b841561084d5761084d6001600160a01b0385163287610a9c565b8161087a5780511561086157805181602001fd5b604051631bb7daad60e11b815260040160405180910390fd5b505050506109c4565b47838110156108a557604051637bdd7ae760e01b815260040160405180910390fd5b5f806001600160a01b0389166108bb3488610eb0565b89896040516108cb929190610ea1565b5f6040518083038185875af1925050503d805f8114610905576040519150601f19603f3d011682016040523d82523d5f602084013e61090a565b606091505b5091509150816109235780511561086157805181602001fd5b83156109c0576040515f90329086908381818185875af1925050503d805f8114610968576040519150601f19603f3d011682016040523d82523d5f602084013e61096d565b606091505b50509050806109be5760405162461bcd60e51b815260206004820152601c60248201527f506f7274616c3a207265696d62757273656d656e74206661696c65640000000060448201526064016101ad565b505b5050505b50506005805460ff60a01b1916600160a01b17905550505050565b604080516001600160a01b038416602482015260448082018490528251808303909101815260649091019091526020810180516001600160e01b031663095ea7b360e01b179052610a308482610ad2565b610513576040516001600160a01b0384811660248301525f6044830152610a9291869182169063095ea7b3906064015b604051602081830303815290604052915060e01b6020820180516001600160e01b038381831617835250505050610b1d565b6105138482610b1d565b6040516001600160a01b03838116602483015260448201839052610acd91859182169063a9059cbb90606401610a60565b505050565b5f5f5f5f60205f8651602088015f8a5af192503d91505f519050828015610b1157508115610b035780600114610b11565b5f866001600160a01b03163b115b93505050505b92915050565b5f5f60205f8451602086015f885af180610b3c576040513d5f823e3d81fd5b50505f513d91508115610b53578060011415610b60565b6001600160a01b0384163b155b1561051357604051635274afe760e01b81526001600160a01b03851660048201526024016101ad565b634e487b7160e01b5f52604160045260245ffd5b60405160a0810167ffffffffffffffff81118282101715610bc057610bc0610b89565b60405290565b6040805190810167ffffffffffffffff81118282101715610bc057610bc0610b89565b803561ffff81168114610bfa575f5ffd5b919050565b6001600160a01b0381168114610c13575f5ffd5b50565b8035610bfa81610bff565b5f5f5f838503610100811215610c35575f5ffd5b60c0811215610c42575f5ffd5b50610c4b610b9d565b843581526020808601359082015260408086013590820152607f85018613610c71575f5ffd5b610c79610bc6565b8060a0870188811115610c8a575f5ffd5b606088015b81811015610ca7578035845260209384019301610c8f565b50816060850152610cb781610be9565b608085015250505080935050610ccf60c08501610c16565b9150610cdd60e08501610c16565b90509250925092565b5f5f60408385031215610cf7575f5ffd5b8235610d0281610bff565b91506020830135610d1281610bff565b809150509250929050565b5f5f5f5f60808587031215610d30575f5ffd5b843593506020850135610d4281610bff565b9250604085013591506060850135610d5981610bff565b939692955090935050565b5f5f5f5f5f5f60a08789031215610d79575f5ffd5b8635610d8481610bff565b9550602087013567ffffffffffffffff811115610d9f575f5ffd5b8701601f81018913610daf575f5ffd5b803567ffffffffffffffff811115610dc5575f5ffd5b896020828401011115610dd6575f5ffd5b6020919091019550935060408701359250610df360608801610c16565b95989497509295919493608090920135925050565b5f60208284031215610e18575f5ffd5b8151610e2381610bff565b9392505050565b5f60c0820190508251825260208301516020830152604083015160408301526060830151606083015f5b6002811015610e73578251825260209283019290910190600101610e54565b50505061ffff60808401511660a083015292915050565b5f60208284031215610e9a575f5ffd5b5051919050565b818382375f9101908152919050565b80820180821115610b1757634e487b7160e01b5f52601160045260245ffdfea26469706673582212202c359c5676bff8bb9be336fb2e0527b4337d8c884e4c9e03679a546f7bda8bea64736f6c634300081c0033
    /// ```
    #[rustfmt::skip]
    #[allow(clippy::all)]
    pub static BYTECODE: alloy_sol_types::private::Bytes = alloy_sol_types::private::Bytes::from_static(
        b"`\x80`@R4\x80\x15`\x0EW__\xFD[P`\x05\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90Ua\x0F\x05\x80a\0/_9_\xF3\xFE`\x80`@R`\x046\x10a\0nW_5`\xE0\x1C\x80cd\x8B\xF7t\x11a\0LW\x80cd\x8B\xF7t\x14a\0\xEDW\x80cv\xFC\xEBv\x14a\x01\x0CW\x80c\xDD\xCE\xAF\xA9\x14a\x01+W\x80c\xE3\xF5\xC5R\x14a\x01JW__\xFD[\x80c\x19\x9CTN\x14a\0rW\x80c?\xB0p'\x14a\0\x93W\x80c]\xC2N\xE3\x14a\0\xCEW[__\xFD[4\x80\x15a\0}W__\xFD[Pa\0\x91a\0\x8C6`\x04a\x0C!V[a\x01]V[\0[4\x80\x15a\0\x9EW__\xFD[P`\x04Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01`@Q\x80\x91\x03\x90\xF3[4\x80\x15a\0\xD9W__\xFD[P`\x03Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[4\x80\x15a\0\xF8W__\xFD[Pa\0\x91a\x01\x076`\x04a\x0C\xE6V[a\x03\xA2V[4\x80\x15a\x01\x17W__\xFD[Pa\0\x91a\x01&6`\x04a\r\x1DV[a\x05\xB9V[4\x80\x15a\x016W__\xFD[P`\x05Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[a\0\x91a\x01X6`\x04a\rdV[a\x06\x8EV[`\x05T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x01\xB6W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01Rv\x14\xDA[\x99\xDB\x19U\\\xD9N\x88\x10[\x1C\x99XY\x1EH\x1D\\\xD9Y`J\x1B`D\x82\x01R`d\x01[`@Q\x80\x91\x03\x90\xFD[_T\x83Q\x14a\x01\xD7W`@Qb\xCB}\xFF`\xE8\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x03\x80T`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`\x01`\x01`\xA0\x1B\x03\x19\x92\x83\x16\x17\x90\x92U`\x04\x80T\x92\x84\x16\x92\x90\x91\x16\x82\x17\x81U` \x85\x01Q`@Qc\x0C\xF9\x9B\xE7`\xE3\x1B\x81R\x91\x82\x01R_\x91\x90cg\xCC\xDF8\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x02BW=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x02f\x91\x90a\x0E\x08V[\x90P`\x01`\x01`\xA0\x1B\x03\x81\x16\x15\x80\x15\x90a\x02\x9DWP`\x01`\x01`\xA0\x1B\x03\x81\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x03$W`\x03T`@\x85\x01Qa\x02\xC2\x91`\x01`\x01`\xA0\x1B\x03\x84\x81\x16\x92\x91\x16\x90a\t\xDFV[`\x03T`@Qc\xBAH\xD1\x17`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90c\xBAH\xD1\x17\x90a\x02\xF2\x90\x87\x90`\x04\x01a\x0E*V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x03\tW__\xFD[PZ\xF1\x15\x80\x15a\x03\x1BW=__>=_\xFD[PPPPa\x03\x89V[`\x03T`@\x80\x86\x01Q\x90Qc\xBAH\xD1\x17`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x92\x16\x91c\xBAH\xD1\x17\x91\x90a\x03Z\x90\x88\x90`\x04\x01a\x0E*V[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x03qW__\xFD[PZ\xF1\x15\x80\x15a\x03\x83W=__>=_\xFD[PPPPP[PP`\x05\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPV[`\x05T`\x01`\x01`\xA0\x1B\x03\x162\x14a\x03\xF4W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x15`$\x82\x01RtPortal: Only recovery`X\x1B`D\x82\x01R`d\x01a\x01\xADV[`\x01`\x01`\xA0\x1B\x03\x81\x16a\x04\x1BW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x80a\x04MWP`\x01`\x01`\xA0\x1B\x03\x82\x16\x15[\x15a\x05\x19WG_\x81\x90\x03a\x04tW`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x82`\x01`\x01`\xA0\x1B\x03\x16\x82`@Q_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x04\xBDW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x04\xC2V[``\x91P[PP\x90P\x80a\x05\x13W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1B`$\x82\x01R\x7FPortal: ETH transfer failed\0\0\0\0\0`D\x82\x01R`d\x01a\x01\xADV[PPPPV[`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x05_W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x05\x83\x91\x90a\x0E\x8AV[\x90P\x80_\x03a\x05\xA5W`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x05\x13`\x01`\x01`\xA0\x1B\x03\x83\x16\x84\x83a\n\x9CV[`\x05T`\x01`\xA8\x1B\x90\x04`\xFF\x16\x15a\x05\xE3W`@Qb\xDC\x14\x9F`\xE4\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90U`\x01`\x01`\xA0\x1B\x03\x81\x16a\x06\x1DW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x83\x15`\x01`\x01`\xA0\x1B\x03\x84\x16\x15\x14\x80a\x067WP\x83\x15\x82\x15\x14[\x15a\x06UW`@Qc\x1DM\xEB\x8B`\xE3\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x93\x90\x93U`\x01\x80T`\x01`\x01`\xA0\x1B\x03\x93\x84\x16`\x01`\x01`\xA0\x1B\x03\x19\x91\x82\x16\x17\x90\x91U`\x02\x91\x90\x91U`\x05\x80T\x92\x90\x93\x16\x91\x16\x17\x90UV[`\x05T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x06\xE2W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01Rv\x14\xDA[\x99\xDB\x19U\\\xD9N\x88\x10[\x1C\x99XY\x1EH\x1D\\\xD9Y`J\x1B`D\x82\x01R`d\x01a\x01\xADV[`\x01`\x01`\xA0\x1B\x03\x82\x16\x15\x80\x15\x90a\x07\x17WP`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x08\x83W`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x07bW=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x07\x86\x91\x90a\x0E\x8AV[\x90P\x84\x81\x10\x15a\x07\xA9W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x07\xBD`\x01`\x01`\xA0\x1B\x03\x83\x16\x89\x87a\t\xDFV[__\x89`\x01`\x01`\xA0\x1B\x03\x164\x8A\x8A`@Qa\x07\xDA\x92\x91\x90a\x0E\xA1V[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x08\x14W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x08\x19V[``\x91P[P\x90\x92P\x90Pa\x083`\x01`\x01`\xA0\x1B\x03\x85\x16\x8B_a\t\xDFV[\x84\x15a\x08MWa\x08M`\x01`\x01`\xA0\x1B\x03\x85\x162\x87a\n\x9CV[\x81a\x08zW\x80Q\x15a\x08aW\x80Q\x81` \x01\xFD[`@Qc\x1B\xB7\xDA\xAD`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[PPPPa\t\xC4V[G\x83\x81\x10\x15a\x08\xA5W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x80`\x01`\x01`\xA0\x1B\x03\x89\x16a\x08\xBB4\x88a\x0E\xB0V[\x89\x89`@Qa\x08\xCB\x92\x91\x90a\x0E\xA1V[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\t\x05W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\t\nV[``\x91P[P\x91P\x91P\x81a\t#W\x80Q\x15a\x08aW\x80Q\x81` \x01\xFD[\x83\x15a\t\xC0W`@Q_\x902\x90\x86\x90\x83\x81\x81\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\thW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\tmV[``\x91P[PP\x90P\x80a\t\xBEW`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1C`$\x82\x01R\x7FPortal: reimbursement failed\0\0\0\0`D\x82\x01R`d\x01a\x01\xADV[P[PPP[PP`\x05\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPPPV[`@\x80Q`\x01`\x01`\xA0\x1B\x03\x84\x16`$\x82\x01R`D\x80\x82\x01\x84\x90R\x82Q\x80\x83\x03\x90\x91\x01\x81R`d\x90\x91\x01\x90\x91R` \x81\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x16c\t^\xA7\xB3`\xE0\x1B\x17\x90Ra\n0\x84\x82a\n\xD2V[a\x05\x13W`@Q`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`$\x83\x01R_`D\x83\x01Ra\n\x92\x91\x86\x91\x82\x16\x90c\t^\xA7\xB3\x90`d\x01[`@Q` \x81\x83\x03\x03\x81R\x90`@R\x91P`\xE0\x1B` \x82\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x83\x81\x83\x16\x17\x83RPPPPa\x0B\x1DV[a\x05\x13\x84\x82a\x0B\x1DV[`@Q`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`$\x83\x01R`D\x82\x01\x83\x90Ra\n\xCD\x91\x85\x91\x82\x16\x90c\xA9\x05\x9C\xBB\x90`d\x01a\n`V[PPPV[____` _\x86Q` \x88\x01_\x8AZ\xF1\x92P=\x91P_Q\x90P\x82\x80\x15a\x0B\x11WP\x81\x15a\x0B\x03W\x80`\x01\x14a\x0B\x11V[_\x86`\x01`\x01`\xA0\x1B\x03\x16;\x11[\x93PPPP[\x92\x91PPV[__` _\x84Q` \x86\x01_\x88Z\xF1\x80a\x0B<W`@Q=_\x82>=\x81\xFD[PP_Q=\x91P\x81\x15a\x0BSW\x80`\x01\x14\x15a\x0B`V[`\x01`\x01`\xA0\x1B\x03\x84\x16;\x15[\x15a\x05\x13W`@QcRt\xAF\xE7`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x85\x16`\x04\x82\x01R`$\x01a\x01\xADV[cNH{q`\xE0\x1B_R`A`\x04R`$_\xFD[`@Q`\xA0\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x0B\xC0Wa\x0B\xC0a\x0B\x89V[`@R\x90V[`@\x80Q\x90\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x0B\xC0Wa\x0B\xC0a\x0B\x89V[\x805a\xFF\xFF\x81\x16\x81\x14a\x0B\xFAW__\xFD[\x91\x90PV[`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x0C\x13W__\xFD[PV[\x805a\x0B\xFA\x81a\x0B\xFFV[___\x83\x85\x03a\x01\0\x81\x12\x15a\x0C5W__\xFD[`\xC0\x81\x12\x15a\x0CBW__\xFD[Pa\x0CKa\x0B\x9DV[\x845\x81R` \x80\x86\x015\x90\x82\x01R`@\x80\x86\x015\x90\x82\x01R`\x7F\x85\x01\x86\x13a\x0CqW__\xFD[a\x0Cya\x0B\xC6V[\x80`\xA0\x87\x01\x88\x81\x11\x15a\x0C\x8AW__\xFD[``\x88\x01[\x81\x81\x10\x15a\x0C\xA7W\x805\x84R` \x93\x84\x01\x93\x01a\x0C\x8FV[P\x81``\x85\x01Ra\x0C\xB7\x81a\x0B\xE9V[`\x80\x85\x01RPPP\x80\x93PPa\x0C\xCF`\xC0\x85\x01a\x0C\x16V[\x91Pa\x0C\xDD`\xE0\x85\x01a\x0C\x16V[\x90P\x92P\x92P\x92V[__`@\x83\x85\x03\x12\x15a\x0C\xF7W__\xFD[\x825a\r\x02\x81a\x0B\xFFV[\x91P` \x83\x015a\r\x12\x81a\x0B\xFFV[\x80\x91PP\x92P\x92\x90PV[____`\x80\x85\x87\x03\x12\x15a\r0W__\xFD[\x845\x93P` \x85\x015a\rB\x81a\x0B\xFFV[\x92P`@\x85\x015\x91P``\x85\x015a\rY\x81a\x0B\xFFV[\x93\x96\x92\x95P\x90\x93PPV[______`\xA0\x87\x89\x03\x12\x15a\ryW__\xFD[\x865a\r\x84\x81a\x0B\xFFV[\x95P` \x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\r\x9FW__\xFD[\x87\x01`\x1F\x81\x01\x89\x13a\r\xAFW__\xFD[\x805g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\r\xC5W__\xFD[\x89` \x82\x84\x01\x01\x11\x15a\r\xD6W__\xFD[` \x91\x90\x91\x01\x95P\x93P`@\x87\x015\x92Pa\r\xF3``\x88\x01a\x0C\x16V[\x95\x98\x94\x97P\x92\x95\x91\x94\x93`\x80\x90\x92\x015\x92PPV[_` \x82\x84\x03\x12\x15a\x0E\x18W__\xFD[\x81Qa\x0E#\x81a\x0B\xFFV[\x93\x92PPPV[_`\xC0\x82\x01\x90P\x82Q\x82R` \x83\x01Q` \x83\x01R`@\x83\x01Q`@\x83\x01R``\x83\x01Q``\x83\x01_[`\x02\x81\x10\x15a\x0EsW\x82Q\x82R` \x92\x83\x01\x92\x90\x91\x01\x90`\x01\x01a\x0ETV[PPPa\xFF\xFF`\x80\x84\x01Q\x16`\xA0\x83\x01R\x92\x91PPV[_` \x82\x84\x03\x12\x15a\x0E\x9AW__\xFD[PQ\x91\x90PV[\x81\x83\x827_\x91\x01\x90\x81R\x91\x90PV[\x80\x82\x01\x80\x82\x11\x15a\x0B\x17WcNH{q`\xE0\x1B_R`\x11`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 ,5\x9CVv\xBF\xF8\xBB\x9B\xE36\xFB.\x05'\xB43}\x8C\x88NL\x9E\x03g\x9ATo{\xDA\x8B\xEAdsolcC\0\x08\x1C\x003",
    );
    /// The runtime bytecode of the contract, as deployed on the network.
    ///
    /// ```text
    ///0x60806040526004361061006e575f3560e01c8063648bf7741161004c578063648bf774146100ed57806376fceb761461010c578063ddceafa91461012b578063e3f5c5521461014a575f5ffd5b8063199c544e146100725780633fb07027146100935780635dc24ee3146100ce575b5f5ffd5b34801561007d575f5ffd5b5061009161008c366004610c21565b61015d565b005b34801561009e575f5ffd5b506004546100b2906001600160a01b031681565b6040516001600160a01b03909116815260200160405180910390f35b3480156100d9575f5ffd5b506003546100b2906001600160a01b031681565b3480156100f8575f5ffd5b50610091610107366004610ce6565b6103a2565b348015610117575f5ffd5b50610091610126366004610d1d565b6105b9565b348015610136575f5ffd5b506005546100b2906001600160a01b031681565b610091610158366004610d64565b61068e565b600554600160a01b900460ff16156101b65760405162461bcd60e51b815260206004820152601760248201527614da5b99db19555cd94e88105b1c9958591e481d5cd959604a1b60448201526064015b60405180910390fd5b5f548351146101d75760405162cb7dff60e81b815260040160405180910390fd5b600380546001600160a01b038481166001600160a01b0319928316179092556004805492841692909116821781556020850151604051630cf99be760e31b8152918201525f91906367ccdf3890602401602060405180830381865afa158015610242573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906102669190610e08565b90506001600160a01b0381161580159061029d57506001600160a01b03811673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b156103245760035460408501516102c2916001600160a01b03848116929116906109df565b60035460405163ba48d11760e01b81526001600160a01b039091169063ba48d117906102f2908790600401610e2a565b5f604051808303815f87803b158015610309575f5ffd5b505af115801561031b573d5f5f3e3d5ffd5b50505050610389565b600354604080860151905163ba48d11760e01b81526001600160a01b039092169163ba48d117919061035a908890600401610e2a565b5f604051808303818588803b158015610371575f5ffd5b505af1158015610383573d5f5f3e3d5ffd5b50505050505b50506005805460ff60a01b1916600160a01b1790555050565b6005546001600160a01b031632146103f45760405162461bcd60e51b8152602060048201526015602482015274506f7274616c3a204f6e6c79207265636f7665727960581b60448201526064016101ad565b6001600160a01b03811661041b5760405163530a10d160e11b815260040160405180910390fd5b6001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee148061044d57506001600160a01b038216155b1561051957475f81900361047457604051636165515360e11b815260040160405180910390fd5b5f826001600160a01b0316826040515f6040518083038185875af1925050503d805f81146104bd576040519150601f19603f3d011682016040523d82523d5f602084013e6104c2565b606091505b50509050806105135760405162461bcd60e51b815260206004820152601b60248201527f506f7274616c3a20455448207472616e73666572206661696c6564000000000060448201526064016101ad565b50505050565b6040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa15801561055f573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906105839190610e8a565b9050805f036105a557604051636165515360e11b815260040160405180910390fd5b6105136001600160a01b0383168483610a9c565b600554600160a81b900460ff16156105e35760405162dc149f60e41b815260040160405180910390fd5b6005805460ff60a81b1916600160a81b1790556001600160a01b03811661061d5760405163530a10d160e11b815260040160405180910390fd5b83156001600160a01b038416151480610637575083158215145b1561065557604051631d4deb8b60e31b815260040160405180910390fd5b5f93909355600180546001600160a01b039384166001600160a01b03199182161790915560029190915560058054929093169116179055565b600554600160a01b900460ff16156106e25760405162461bcd60e51b815260206004820152601760248201527614da5b99db19555cd94e88105b1c9958591e481d5cd959604a1b60448201526064016101ad565b6001600160a01b0382161580159061071757506001600160a01b03821673eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee14155b15610883576040516370a0823160e01b815230600482015282905f906001600160a01b038316906370a0823190602401602060405180830381865afa158015610762573d5f5f3e3d5ffd5b505050506040513d601f19601f820116820180604052508101906107869190610e8a565b9050848110156107a957604051637bdd7ae760e01b815260040160405180910390fd5b6107bd6001600160a01b03831689876109df565b5f5f896001600160a01b0316348a8a6040516107da929190610ea1565b5f6040518083038185875af1925050503d805f8114610814576040519150601f19603f3d011682016040523d82523d5f602084013e610819565b606091505b5090925090506108336001600160a01b0385168b5f6109df565b841561084d5761084d6001600160a01b0385163287610a9c565b8161087a5780511561086157805181602001fd5b604051631bb7daad60e11b815260040160405180910390fd5b505050506109c4565b47838110156108a557604051637bdd7ae760e01b815260040160405180910390fd5b5f806001600160a01b0389166108bb3488610eb0565b89896040516108cb929190610ea1565b5f6040518083038185875af1925050503d805f8114610905576040519150601f19603f3d011682016040523d82523d5f602084013e61090a565b606091505b5091509150816109235780511561086157805181602001fd5b83156109c0576040515f90329086908381818185875af1925050503d805f8114610968576040519150601f19603f3d011682016040523d82523d5f602084013e61096d565b606091505b50509050806109be5760405162461bcd60e51b815260206004820152601c60248201527f506f7274616c3a207265696d62757273656d656e74206661696c65640000000060448201526064016101ad565b505b5050505b50506005805460ff60a01b1916600160a01b17905550505050565b604080516001600160a01b038416602482015260448082018490528251808303909101815260649091019091526020810180516001600160e01b031663095ea7b360e01b179052610a308482610ad2565b610513576040516001600160a01b0384811660248301525f6044830152610a9291869182169063095ea7b3906064015b604051602081830303815290604052915060e01b6020820180516001600160e01b038381831617835250505050610b1d565b6105138482610b1d565b6040516001600160a01b03838116602483015260448201839052610acd91859182169063a9059cbb90606401610a60565b505050565b5f5f5f5f60205f8651602088015f8a5af192503d91505f519050828015610b1157508115610b035780600114610b11565b5f866001600160a01b03163b115b93505050505b92915050565b5f5f60205f8451602086015f885af180610b3c576040513d5f823e3d81fd5b50505f513d91508115610b53578060011415610b60565b6001600160a01b0384163b155b1561051357604051635274afe760e01b81526001600160a01b03851660048201526024016101ad565b634e487b7160e01b5f52604160045260245ffd5b60405160a0810167ffffffffffffffff81118282101715610bc057610bc0610b89565b60405290565b6040805190810167ffffffffffffffff81118282101715610bc057610bc0610b89565b803561ffff81168114610bfa575f5ffd5b919050565b6001600160a01b0381168114610c13575f5ffd5b50565b8035610bfa81610bff565b5f5f5f838503610100811215610c35575f5ffd5b60c0811215610c42575f5ffd5b50610c4b610b9d565b843581526020808601359082015260408086013590820152607f85018613610c71575f5ffd5b610c79610bc6565b8060a0870188811115610c8a575f5ffd5b606088015b81811015610ca7578035845260209384019301610c8f565b50816060850152610cb781610be9565b608085015250505080935050610ccf60c08501610c16565b9150610cdd60e08501610c16565b90509250925092565b5f5f60408385031215610cf7575f5ffd5b8235610d0281610bff565b91506020830135610d1281610bff565b809150509250929050565b5f5f5f5f60808587031215610d30575f5ffd5b843593506020850135610d4281610bff565b9250604085013591506060850135610d5981610bff565b939692955090935050565b5f5f5f5f5f5f60a08789031215610d79575f5ffd5b8635610d8481610bff565b9550602087013567ffffffffffffffff811115610d9f575f5ffd5b8701601f81018913610daf575f5ffd5b803567ffffffffffffffff811115610dc5575f5ffd5b896020828401011115610dd6575f5ffd5b6020919091019550935060408701359250610df360608801610c16565b95989497509295919493608090920135925050565b5f60208284031215610e18575f5ffd5b8151610e2381610bff565b9392505050565b5f60c0820190508251825260208301516020830152604083015160408301526060830151606083015f5b6002811015610e73578251825260209283019290910190600101610e54565b50505061ffff60808401511660a083015292915050565b5f60208284031215610e9a575f5ffd5b5051919050565b818382375f9101908152919050565b80820180821115610b1757634e487b7160e01b5f52601160045260245ffdfea26469706673582212202c359c5676bff8bb9be336fb2e0527b4337d8c884e4c9e03679a546f7bda8bea64736f6c634300081c0033
    /// ```
    #[rustfmt::skip]
    #[allow(clippy::all)]
    pub static DEPLOYED_BYTECODE: alloy_sol_types::private::Bytes = alloy_sol_types::private::Bytes::from_static(
        b"`\x80`@R`\x046\x10a\0nW_5`\xE0\x1C\x80cd\x8B\xF7t\x11a\0LW\x80cd\x8B\xF7t\x14a\0\xEDW\x80cv\xFC\xEBv\x14a\x01\x0CW\x80c\xDD\xCE\xAF\xA9\x14a\x01+W\x80c\xE3\xF5\xC5R\x14a\x01JW__\xFD[\x80c\x19\x9CTN\x14a\0rW\x80c?\xB0p'\x14a\0\x93W\x80c]\xC2N\xE3\x14a\0\xCEW[__\xFD[4\x80\x15a\0}W__\xFD[Pa\0\x91a\0\x8C6`\x04a\x0C!V[a\x01]V[\0[4\x80\x15a\0\x9EW__\xFD[P`\x04Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01`@Q\x80\x91\x03\x90\xF3[4\x80\x15a\0\xD9W__\xFD[P`\x03Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[4\x80\x15a\0\xF8W__\xFD[Pa\0\x91a\x01\x076`\x04a\x0C\xE6V[a\x03\xA2V[4\x80\x15a\x01\x17W__\xFD[Pa\0\x91a\x01&6`\x04a\r\x1DV[a\x05\xB9V[4\x80\x15a\x016W__\xFD[P`\x05Ta\0\xB2\x90`\x01`\x01`\xA0\x1B\x03\x16\x81V[a\0\x91a\x01X6`\x04a\rdV[a\x06\x8EV[`\x05T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x01\xB6W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01Rv\x14\xDA[\x99\xDB\x19U\\\xD9N\x88\x10[\x1C\x99XY\x1EH\x1D\\\xD9Y`J\x1B`D\x82\x01R`d\x01[`@Q\x80\x91\x03\x90\xFD[_T\x83Q\x14a\x01\xD7W`@Qb\xCB}\xFF`\xE8\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x03\x80T`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`\x01`\x01`\xA0\x1B\x03\x19\x92\x83\x16\x17\x90\x92U`\x04\x80T\x92\x84\x16\x92\x90\x91\x16\x82\x17\x81U` \x85\x01Q`@Qc\x0C\xF9\x9B\xE7`\xE3\x1B\x81R\x91\x82\x01R_\x91\x90cg\xCC\xDF8\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x02BW=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x02f\x91\x90a\x0E\x08V[\x90P`\x01`\x01`\xA0\x1B\x03\x81\x16\x15\x80\x15\x90a\x02\x9DWP`\x01`\x01`\xA0\x1B\x03\x81\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x03$W`\x03T`@\x85\x01Qa\x02\xC2\x91`\x01`\x01`\xA0\x1B\x03\x84\x81\x16\x92\x91\x16\x90a\t\xDFV[`\x03T`@Qc\xBAH\xD1\x17`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90c\xBAH\xD1\x17\x90a\x02\xF2\x90\x87\x90`\x04\x01a\x0E*V[_`@Q\x80\x83\x03\x81_\x87\x80;\x15\x80\x15a\x03\tW__\xFD[PZ\xF1\x15\x80\x15a\x03\x1BW=__>=_\xFD[PPPPa\x03\x89V[`\x03T`@\x80\x86\x01Q\x90Qc\xBAH\xD1\x17`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x92\x16\x91c\xBAH\xD1\x17\x91\x90a\x03Z\x90\x88\x90`\x04\x01a\x0E*V[_`@Q\x80\x83\x03\x81\x85\x88\x80;\x15\x80\x15a\x03qW__\xFD[PZ\xF1\x15\x80\x15a\x03\x83W=__>=_\xFD[PPPPP[PP`\x05\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPV[`\x05T`\x01`\x01`\xA0\x1B\x03\x162\x14a\x03\xF4W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x15`$\x82\x01RtPortal: Only recovery`X\x1B`D\x82\x01R`d\x01a\x01\xADV[`\x01`\x01`\xA0\x1B\x03\x81\x16a\x04\x1BW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x80a\x04MWP`\x01`\x01`\xA0\x1B\x03\x82\x16\x15[\x15a\x05\x19WG_\x81\x90\x03a\x04tW`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x82`\x01`\x01`\xA0\x1B\x03\x16\x82`@Q_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x04\xBDW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x04\xC2V[``\x91P[PP\x90P\x80a\x05\x13W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1B`$\x82\x01R\x7FPortal: ETH transfer failed\0\0\0\0\0`D\x82\x01R`d\x01a\x01\xADV[PPPPV[`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x05_W=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x05\x83\x91\x90a\x0E\x8AV[\x90P\x80_\x03a\x05\xA5W`@QcaeQS`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x05\x13`\x01`\x01`\xA0\x1B\x03\x83\x16\x84\x83a\n\x9CV[`\x05T`\x01`\xA8\x1B\x90\x04`\xFF\x16\x15a\x05\xE3W`@Qb\xDC\x14\x9F`\xE4\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\x05\x80T`\xFF`\xA8\x1B\x19\x16`\x01`\xA8\x1B\x17\x90U`\x01`\x01`\xA0\x1B\x03\x81\x16a\x06\x1DW`@QcS\n\x10\xD1`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[\x83\x15`\x01`\x01`\xA0\x1B\x03\x84\x16\x15\x14\x80a\x067WP\x83\x15\x82\x15\x14[\x15a\x06UW`@Qc\x1DM\xEB\x8B`\xE3\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x93\x90\x93U`\x01\x80T`\x01`\x01`\xA0\x1B\x03\x93\x84\x16`\x01`\x01`\xA0\x1B\x03\x19\x91\x82\x16\x17\x90\x91U`\x02\x91\x90\x91U`\x05\x80T\x92\x90\x93\x16\x91\x16\x17\x90UV[`\x05T`\x01`\xA0\x1B\x90\x04`\xFF\x16\x15a\x06\xE2W`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x17`$\x82\x01Rv\x14\xDA[\x99\xDB\x19U\\\xD9N\x88\x10[\x1C\x99XY\x1EH\x1D\\\xD9Y`J\x1B`D\x82\x01R`d\x01a\x01\xADV[`\x01`\x01`\xA0\x1B\x03\x82\x16\x15\x80\x15\x90a\x07\x17WP`\x01`\x01`\xA0\x1B\x03\x82\x16s\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\xEE\x14\x15[\x15a\x08\x83W`@Qcp\xA0\x821`\xE0\x1B\x81R0`\x04\x82\x01R\x82\x90_\x90`\x01`\x01`\xA0\x1B\x03\x83\x16\x90cp\xA0\x821\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x07bW=__>=_\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x07\x86\x91\x90a\x0E\x8AV[\x90P\x84\x81\x10\x15a\x07\xA9W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x07\xBD`\x01`\x01`\xA0\x1B\x03\x83\x16\x89\x87a\t\xDFV[__\x89`\x01`\x01`\xA0\x1B\x03\x164\x8A\x8A`@Qa\x07\xDA\x92\x91\x90a\x0E\xA1V[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\x08\x14W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\x08\x19V[``\x91P[P\x90\x92P\x90Pa\x083`\x01`\x01`\xA0\x1B\x03\x85\x16\x8B_a\t\xDFV[\x84\x15a\x08MWa\x08M`\x01`\x01`\xA0\x1B\x03\x85\x162\x87a\n\x9CV[\x81a\x08zW\x80Q\x15a\x08aW\x80Q\x81` \x01\xFD[`@Qc\x1B\xB7\xDA\xAD`\xE1\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[PPPPa\t\xC4V[G\x83\x81\x10\x15a\x08\xA5W`@Qc{\xDDz\xE7`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[_\x80`\x01`\x01`\xA0\x1B\x03\x89\x16a\x08\xBB4\x88a\x0E\xB0V[\x89\x89`@Qa\x08\xCB\x92\x91\x90a\x0E\xA1V[_`@Q\x80\x83\x03\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\t\x05W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\t\nV[``\x91P[P\x91P\x91P\x81a\t#W\x80Q\x15a\x08aW\x80Q\x81` \x01\xFD[\x83\x15a\t\xC0W`@Q_\x902\x90\x86\x90\x83\x81\x81\x81\x85\x87Z\xF1\x92PPP=\x80_\x81\x14a\thW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=_` \x84\x01>a\tmV[``\x91P[PP\x90P\x80a\t\xBEW`@QbF\x1B\xCD`\xE5\x1B\x81R` `\x04\x82\x01R`\x1C`$\x82\x01R\x7FPortal: reimbursement failed\0\0\0\0`D\x82\x01R`d\x01a\x01\xADV[P[PPP[PP`\x05\x80T`\xFF`\xA0\x1B\x19\x16`\x01`\xA0\x1B\x17\x90UPPPPV[`@\x80Q`\x01`\x01`\xA0\x1B\x03\x84\x16`$\x82\x01R`D\x80\x82\x01\x84\x90R\x82Q\x80\x83\x03\x90\x91\x01\x81R`d\x90\x91\x01\x90\x91R` \x81\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x16c\t^\xA7\xB3`\xE0\x1B\x17\x90Ra\n0\x84\x82a\n\xD2V[a\x05\x13W`@Q`\x01`\x01`\xA0\x1B\x03\x84\x81\x16`$\x83\x01R_`D\x83\x01Ra\n\x92\x91\x86\x91\x82\x16\x90c\t^\xA7\xB3\x90`d\x01[`@Q` \x81\x83\x03\x03\x81R\x90`@R\x91P`\xE0\x1B` \x82\x01\x80Q`\x01`\x01`\xE0\x1B\x03\x83\x81\x83\x16\x17\x83RPPPPa\x0B\x1DV[a\x05\x13\x84\x82a\x0B\x1DV[`@Q`\x01`\x01`\xA0\x1B\x03\x83\x81\x16`$\x83\x01R`D\x82\x01\x83\x90Ra\n\xCD\x91\x85\x91\x82\x16\x90c\xA9\x05\x9C\xBB\x90`d\x01a\n`V[PPPV[____` _\x86Q` \x88\x01_\x8AZ\xF1\x92P=\x91P_Q\x90P\x82\x80\x15a\x0B\x11WP\x81\x15a\x0B\x03W\x80`\x01\x14a\x0B\x11V[_\x86`\x01`\x01`\xA0\x1B\x03\x16;\x11[\x93PPPP[\x92\x91PPV[__` _\x84Q` \x86\x01_\x88Z\xF1\x80a\x0B<W`@Q=_\x82>=\x81\xFD[PP_Q=\x91P\x81\x15a\x0BSW\x80`\x01\x14\x15a\x0B`V[`\x01`\x01`\xA0\x1B\x03\x84\x16;\x15[\x15a\x05\x13W`@QcRt\xAF\xE7`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x85\x16`\x04\x82\x01R`$\x01a\x01\xADV[cNH{q`\xE0\x1B_R`A`\x04R`$_\xFD[`@Q`\xA0\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x0B\xC0Wa\x0B\xC0a\x0B\x89V[`@R\x90V[`@\x80Q\x90\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x0B\xC0Wa\x0B\xC0a\x0B\x89V[\x805a\xFF\xFF\x81\x16\x81\x14a\x0B\xFAW__\xFD[\x91\x90PV[`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x0C\x13W__\xFD[PV[\x805a\x0B\xFA\x81a\x0B\xFFV[___\x83\x85\x03a\x01\0\x81\x12\x15a\x0C5W__\xFD[`\xC0\x81\x12\x15a\x0CBW__\xFD[Pa\x0CKa\x0B\x9DV[\x845\x81R` \x80\x86\x015\x90\x82\x01R`@\x80\x86\x015\x90\x82\x01R`\x7F\x85\x01\x86\x13a\x0CqW__\xFD[a\x0Cya\x0B\xC6V[\x80`\xA0\x87\x01\x88\x81\x11\x15a\x0C\x8AW__\xFD[``\x88\x01[\x81\x81\x10\x15a\x0C\xA7W\x805\x84R` \x93\x84\x01\x93\x01a\x0C\x8FV[P\x81``\x85\x01Ra\x0C\xB7\x81a\x0B\xE9V[`\x80\x85\x01RPPP\x80\x93PPa\x0C\xCF`\xC0\x85\x01a\x0C\x16V[\x91Pa\x0C\xDD`\xE0\x85\x01a\x0C\x16V[\x90P\x92P\x92P\x92V[__`@\x83\x85\x03\x12\x15a\x0C\xF7W__\xFD[\x825a\r\x02\x81a\x0B\xFFV[\x91P` \x83\x015a\r\x12\x81a\x0B\xFFV[\x80\x91PP\x92P\x92\x90PV[____`\x80\x85\x87\x03\x12\x15a\r0W__\xFD[\x845\x93P` \x85\x015a\rB\x81a\x0B\xFFV[\x92P`@\x85\x015\x91P``\x85\x015a\rY\x81a\x0B\xFFV[\x93\x96\x92\x95P\x90\x93PPV[______`\xA0\x87\x89\x03\x12\x15a\ryW__\xFD[\x865a\r\x84\x81a\x0B\xFFV[\x95P` \x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\r\x9FW__\xFD[\x87\x01`\x1F\x81\x01\x89\x13a\r\xAFW__\xFD[\x805g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\r\xC5W__\xFD[\x89` \x82\x84\x01\x01\x11\x15a\r\xD6W__\xFD[` \x91\x90\x91\x01\x95P\x93P`@\x87\x015\x92Pa\r\xF3``\x88\x01a\x0C\x16V[\x95\x98\x94\x97P\x92\x95\x91\x94\x93`\x80\x90\x92\x015\x92PPV[_` \x82\x84\x03\x12\x15a\x0E\x18W__\xFD[\x81Qa\x0E#\x81a\x0B\xFFV[\x93\x92PPPV[_`\xC0\x82\x01\x90P\x82Q\x82R` \x83\x01Q` \x83\x01R`@\x83\x01Q`@\x83\x01R``\x83\x01Q``\x83\x01_[`\x02\x81\x10\x15a\x0EsW\x82Q\x82R` \x92\x83\x01\x92\x90\x91\x01\x90`\x01\x01a\x0ETV[PPPa\xFF\xFF`\x80\x84\x01Q\x16`\xA0\x83\x01R\x92\x91PPV[_` \x82\x84\x03\x12\x15a\x0E\x9AW__\xFD[PQ\x91\x90PV[\x81\x83\x827_\x91\x01\x90\x81R\x91\x90PV[\x80\x82\x01\x80\x82\x11\x15a\x0B\x17WcNH{q`\xE0\x1B_R`\x11`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 ,5\x9CVv\xBF\xF8\xBB\x9B\xE36\xFB.\x05'\xB43}\x8C\x88NL\x9E\x03g\x9ATo{\xDA\x8B\xEAdsolcC\0\x08\x1C\x003",
    );
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Custom error with signature `AlreadyInitialized()` and selector `0x0dc149f0`.
```solidity
error AlreadyInitialized();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct AlreadyInitialized;
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
        impl ::core::convert::From<AlreadyInitialized> for UnderlyingRustTuple<'_> {
            fn from(value: AlreadyInitialized) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for AlreadyInitialized {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for AlreadyInitialized {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "AlreadyInitialized()";
            const SELECTOR: [u8; 4] = [13u8, 193u8, 73u8, 240u8];
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
    /**Custom error with signature `BridgeCallFailed()` and selector `0x376fb55a`.
```solidity
error BridgeCallFailed();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct BridgeCallFailed;
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
        impl ::core::convert::From<BridgeCallFailed> for UnderlyingRustTuple<'_> {
            fn from(value: BridgeCallFailed) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for BridgeCallFailed {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for BridgeCallFailed {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "BridgeCallFailed()";
            const SELECTOR: [u8; 4] = [55u8, 111u8, 181u8, 90u8];
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
    /**Custom error with signature `InsufficientBalanceForLiFiBridging()` and selector `0x7bdd7ae7`.
```solidity
error InsufficientBalanceForLiFiBridging();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InsufficientBalanceForLiFiBridging;
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
        impl ::core::convert::From<InsufficientBalanceForLiFiBridging>
        for UnderlyingRustTuple<'_> {
            fn from(value: InsufficientBalanceForLiFiBridging) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for InsufficientBalanceForLiFiBridging {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InsufficientBalanceForLiFiBridging {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InsufficientBalanceForLiFiBridging()";
            const SELECTOR: [u8; 4] = [123u8, 221u8, 122u8, 231u8];
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
    /**Custom error with signature `InvalidLiFiAddress()` and selector `0xa4b19f98`.
```solidity
error InvalidLiFiAddress();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidLiFiAddress;
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
        impl ::core::convert::From<InvalidLiFiAddress> for UnderlyingRustTuple<'_> {
            fn from(value: InvalidLiFiAddress) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for InvalidLiFiAddress {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidLiFiAddress {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidLiFiAddress()";
            const SELECTOR: [u8; 4] = [164u8, 177u8, 159u8, 152u8];
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
    /**Custom error with signature `InvalidOwnerHash()` and selector `0xcb7dff00`.
```solidity
error InvalidOwnerHash();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidOwnerHash;
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
        impl ::core::convert::From<InvalidOwnerHash> for UnderlyingRustTuple<'_> {
            fn from(value: InvalidOwnerHash) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for InvalidOwnerHash {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidOwnerHash {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidOwnerHash()";
            const SELECTOR: [u8; 4] = [203u8, 125u8, 255u8, 0u8];
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
    /**Custom error with signature `InvalidOwnerHashOrExitBridgeData()` and selector `0xea6f5c58`.
```solidity
error InvalidOwnerHashOrExitBridgeData();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidOwnerHashOrExitBridgeData;
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
        impl ::core::convert::From<InvalidOwnerHashOrExitBridgeData>
        for UnderlyingRustTuple<'_> {
            fn from(value: InvalidOwnerHashOrExitBridgeData) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for InvalidOwnerHashOrExitBridgeData {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidOwnerHashOrExitBridgeData {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidOwnerHashOrExitBridgeData()";
            const SELECTOR: [u8; 4] = [234u8, 111u8, 92u8, 88u8];
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
    /**Custom error with signature `InvalidRecoveryAddress()` and selector `0xa61421a2`.
```solidity
error InvalidRecoveryAddress();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidRecoveryAddress;
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
        impl ::core::convert::From<InvalidRecoveryAddress> for UnderlyingRustTuple<'_> {
            fn from(value: InvalidRecoveryAddress) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for InvalidRecoveryAddress {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidRecoveryAddress {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidRecoveryAddress()";
            const SELECTOR: [u8; 4] = [166u8, 20u8, 33u8, 162u8];
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
    /**Custom error with signature `InvalidSignatureOrTamperedData()` and selector `0xe900337e`.
```solidity
error InvalidSignatureOrTamperedData();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct InvalidSignatureOrTamperedData;
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
        impl ::core::convert::From<InvalidSignatureOrTamperedData>
        for UnderlyingRustTuple<'_> {
            fn from(value: InvalidSignatureOrTamperedData) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for InvalidSignatureOrTamperedData {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for InvalidSignatureOrTamperedData {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "InvalidSignatureOrTamperedData()";
            const SELECTOR: [u8; 4] = [233u8, 0u8, 51u8, 126u8];
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
    /**Custom error with signature `NoBalance()` and selector `0xc2caa2a6`.
```solidity
error NoBalance();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct NoBalance;
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
        impl ::core::convert::From<NoBalance> for UnderlyingRustTuple<'_> {
            fn from(value: NoBalance) -> Self {
                ()
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>> for NoBalance {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for NoBalance {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "NoBalance()";
            const SELECTOR: [u8; 4] = [194u8, 202u8, 162u8, 166u8];
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
    /**Custom error with signature `SafeERC20FailedOperation(address)` and selector `0x5274afe7`.
```solidity
error SafeERC20FailedOperation(address token);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct SafeERC20FailedOperation {
        #[allow(missing_docs)]
        pub token: alloy::sol_types::private::Address,
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
        impl ::core::convert::From<SafeERC20FailedOperation>
        for UnderlyingRustTuple<'_> {
            fn from(value: SafeERC20FailedOperation) -> Self {
                (value.token,)
            }
        }
        #[automatically_derived]
        #[doc(hidden)]
        impl ::core::convert::From<UnderlyingRustTuple<'_>>
        for SafeERC20FailedOperation {
            fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                Self { token: tuple.0 }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolError for SafeERC20FailedOperation {
            type Parameters<'a> = UnderlyingSolTuple<'a>;
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "SafeERC20FailedOperation(address)";
            const SELECTOR: [u8; 4] = [82u8, 116u8, 175u8, 231u8];
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
                        &self.token,
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
    /**Constructor`.
```solidity
constructor();
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct constructorCall {}
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
            impl ::core::convert::From<constructorCall> for UnderlyingRustTuple<'_> {
                fn from(value: constructorCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for constructorCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolConstructor for constructorCall {
            type Parameters<'a> = ();
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
                ()
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `bridge(address,bytes,uint256,address,uint256)` and selector `0xe3f5c552`.
```solidity
function bridge(address lifiDiamondAddress, bytes memory bridgeData, uint256 amount, address currency, uint256 gasFee) external payable;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct bridgeCall {
        #[allow(missing_docs)]
        pub lifiDiamondAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub bridgeData: alloy::sol_types::private::Bytes,
        #[allow(missing_docs)]
        pub amount: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub currency: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub gasFee: alloy::sol_types::private::primitives::aliases::U256,
    }
    ///Container type for the return parameters of the [`bridge(address,bytes,uint256,address,uint256)`](bridgeCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct bridgeReturn {}
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
                alloy::sol_types::sol_data::Bytes,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::Address,
                alloy::sol_types::private::Bytes,
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
            impl ::core::convert::From<bridgeCall> for UnderlyingRustTuple<'_> {
                fn from(value: bridgeCall) -> Self {
                    (
                        value.lifiDiamondAddress,
                        value.bridgeData,
                        value.amount,
                        value.currency,
                        value.gasFee,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for bridgeCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        lifiDiamondAddress: tuple.0,
                        bridgeData: tuple.1,
                        amount: tuple.2,
                        currency: tuple.3,
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
            impl ::core::convert::From<bridgeReturn> for UnderlyingRustTuple<'_> {
                fn from(value: bridgeReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for bridgeReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl bridgeReturn {
            fn _tokenize(
                &self,
            ) -> <bridgeCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for bridgeCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Bytes,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = bridgeReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "bridge(address,bytes,uint256,address,uint256)";
            const SELECTOR: [u8; 4] = [227u8, 245u8, 197u8, 82u8];
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
                        &self.lifiDiamondAddress,
                    ),
                    <alloy::sol_types::sol_data::Bytes as alloy_sol_types::SolType>::tokenize(
                        &self.bridgeData,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.amount),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.currency,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.gasFee),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                bridgeReturn::_tokenize(ret)
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
    /**Function with signature `curvyAggregator()` and selector `0x5dc24ee3`.
```solidity
function curvyAggregator() external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct curvyAggregatorCall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`curvyAggregator()`](curvyAggregatorCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct curvyAggregatorReturn {
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
            impl ::core::convert::From<curvyAggregatorCall> for UnderlyingRustTuple<'_> {
                fn from(value: curvyAggregatorCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for curvyAggregatorCall {
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
            impl ::core::convert::From<curvyAggregatorReturn>
            for UnderlyingRustTuple<'_> {
                fn from(value: curvyAggregatorReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>>
            for curvyAggregatorReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for curvyAggregatorCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "curvyAggregator()";
            const SELECTOR: [u8; 4] = [93u8, 194u8, 78u8, 227u8];
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
                        let r: curvyAggregatorReturn = r.into();
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
                        let r: curvyAggregatorReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `curvyVault()` and selector `0x3fb07027`.
```solidity
function curvyVault() external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct curvyVaultCall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`curvyVault()`](curvyVaultCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct curvyVaultReturn {
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
            impl ::core::convert::From<curvyVaultCall> for UnderlyingRustTuple<'_> {
                fn from(value: curvyVaultCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for curvyVaultCall {
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
            impl ::core::convert::From<curvyVaultReturn> for UnderlyingRustTuple<'_> {
                fn from(value: curvyVaultReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for curvyVaultReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for curvyVaultCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "curvyVault()";
            const SELECTOR: [u8; 4] = [63u8, 176u8, 112u8, 39u8];
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
                        let r: curvyVaultReturn = r.into();
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
                        let r: curvyVaultReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `initialize(uint256,address,uint256,address)` and selector `0x76fceb76`.
```solidity
function initialize(uint256 ownerHash, address exitAddress, uint256 exitChainId, address _recovery) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct initializeCall {
        #[allow(missing_docs)]
        pub ownerHash: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub exitAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub exitChainId: alloy::sol_types::private::primitives::aliases::U256,
        #[allow(missing_docs)]
        pub _recovery: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`initialize(uint256,address,uint256,address)`](initializeCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct initializeReturn {}
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
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                alloy::sol_types::private::primitives::aliases::U256,
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
            impl ::core::convert::From<initializeCall> for UnderlyingRustTuple<'_> {
                fn from(value: initializeCall) -> Self {
                    (
                        value.ownerHash,
                        value.exitAddress,
                        value.exitChainId,
                        value._recovery,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for initializeCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        ownerHash: tuple.0,
                        exitAddress: tuple.1,
                        exitChainId: tuple.2,
                        _recovery: tuple.3,
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
            impl ::core::convert::From<initializeReturn> for UnderlyingRustTuple<'_> {
                fn from(value: initializeReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for initializeReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl initializeReturn {
            fn _tokenize(
                &self,
            ) -> <initializeCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for initializeCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Uint<256>,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = initializeReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "initialize(uint256,address,uint256,address)";
            const SELECTOR: [u8; 4] = [118u8, 252u8, 235u8, 118u8];
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
                        &self.exitAddress,
                    ),
                    <alloy::sol_types::sol_data::Uint<
                        256,
                    > as alloy_sol_types::SolType>::tokenize(&self.exitChainId),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self._recovery,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                initializeReturn::_tokenize(ret)
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
    /**Function with signature `recover(address,address)` and selector `0x648bf774`.
```solidity
function recover(address tokenAddress, address to) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct recoverCall {
        #[allow(missing_docs)]
        pub tokenAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub to: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`recover(address,address)`](recoverCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct recoverReturn {}
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
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
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
            impl ::core::convert::From<recoverCall> for UnderlyingRustTuple<'_> {
                fn from(value: recoverCall) -> Self {
                    (value.tokenAddress, value.to)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for recoverCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        tokenAddress: tuple.0,
                        to: tuple.1,
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
            impl ::core::convert::From<recoverReturn> for UnderlyingRustTuple<'_> {
                fn from(value: recoverReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for recoverReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl recoverReturn {
            fn _tokenize(
                &self,
            ) -> <recoverCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for recoverCall {
            type Parameters<'a> = (
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = recoverReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "recover(address,address)";
            const SELECTOR: [u8; 4] = [100u8, 139u8, 247u8, 116u8];
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
                        &self.tokenAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.to,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                recoverReturn::_tokenize(ret)
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
    /**Function with signature `recovery()` and selector `0xddceafa9`.
```solidity
function recovery() external view returns (address);
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct recoveryCall;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    ///Container type for the return parameters of the [`recovery()`](recoveryCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct recoveryReturn {
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
            impl ::core::convert::From<recoveryCall> for UnderlyingRustTuple<'_> {
                fn from(value: recoveryCall) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for recoveryCall {
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
            impl ::core::convert::From<recoveryReturn> for UnderlyingRustTuple<'_> {
                fn from(value: recoveryReturn) -> Self {
                    (value._0,)
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for recoveryReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self { _0: tuple.0 }
                }
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for recoveryCall {
            type Parameters<'a> = ();
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = alloy::sol_types::private::Address;
            type ReturnTuple<'a> = (alloy::sol_types::sol_data::Address,);
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "recovery()";
            const SELECTOR: [u8; 4] = [221u8, 206u8, 175u8, 169u8];
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
                        let r: recoveryReturn = r.into();
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
                        let r: recoveryReturn = r.into();
                        r._0
                    })
            }
        }
    };
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Default, Debug, PartialEq, Eq, Hash)]
    /**Function with signature `shield((uint256,uint256,uint256,uint256[2],uint16),address,address)` and selector `0x199c544e`.
```solidity
function shield(CurvyTypes.Note memory note, address curvyAggregatorAlphaProxyAddress, address curvyVaultProxyAddress) external;
```*/
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct shieldCall {
        #[allow(missing_docs)]
        pub note: <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
        #[allow(missing_docs)]
        pub curvyAggregatorAlphaProxyAddress: alloy::sol_types::private::Address,
        #[allow(missing_docs)]
        pub curvyVaultProxyAddress: alloy::sol_types::private::Address,
    }
    ///Container type for the return parameters of the [`shield((uint256,uint256,uint256,uint256[2],uint16),address,address)`](shieldCall) function.
    #[allow(non_camel_case_types, non_snake_case, clippy::pub_underscore_fields)]
    #[derive(Clone)]
    pub struct shieldReturn {}
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
                alloy::sol_types::sol_data::Address,
            );
            #[doc(hidden)]
            type UnderlyingRustTuple<'a> = (
                <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
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
            impl ::core::convert::From<shieldCall> for UnderlyingRustTuple<'_> {
                fn from(value: shieldCall) -> Self {
                    (
                        value.note,
                        value.curvyAggregatorAlphaProxyAddress,
                        value.curvyVaultProxyAddress,
                    )
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for shieldCall {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {
                        note: tuple.0,
                        curvyAggregatorAlphaProxyAddress: tuple.1,
                        curvyVaultProxyAddress: tuple.2,
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
            impl ::core::convert::From<shieldReturn> for UnderlyingRustTuple<'_> {
                fn from(value: shieldReturn) -> Self {
                    ()
                }
            }
            #[automatically_derived]
            #[doc(hidden)]
            impl ::core::convert::From<UnderlyingRustTuple<'_>> for shieldReturn {
                fn from(tuple: UnderlyingRustTuple<'_>) -> Self {
                    Self {}
                }
            }
        }
        impl shieldReturn {
            fn _tokenize(
                &self,
            ) -> <shieldCall as alloy_sol_types::SolCall>::ReturnToken<'_> {
                ()
            }
        }
        #[automatically_derived]
        impl alloy_sol_types::SolCall for shieldCall {
            type Parameters<'a> = (
                CurvyTypes::Note,
                alloy::sol_types::sol_data::Address,
                alloy::sol_types::sol_data::Address,
            );
            type Token<'a> = <Self::Parameters<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            type Return = shieldReturn;
            type ReturnTuple<'a> = ();
            type ReturnToken<'a> = <Self::ReturnTuple<
                'a,
            > as alloy_sol_types::SolType>::Token<'a>;
            const SIGNATURE: &'static str = "shield((uint256,uint256,uint256,uint256[2],uint16),address,address)";
            const SELECTOR: [u8; 4] = [25u8, 156u8, 84u8, 78u8];
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
                        &self.curvyAggregatorAlphaProxyAddress,
                    ),
                    <alloy::sol_types::sol_data::Address as alloy_sol_types::SolType>::tokenize(
                        &self.curvyVaultProxyAddress,
                    ),
                )
            }
            #[inline]
            fn tokenize_returns(ret: &Self::Return) -> Self::ReturnToken<'_> {
                shieldReturn::_tokenize(ret)
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
    ///Container for all the [`Portal`](self) function calls.
    #[derive(Clone)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive()]
    pub enum PortalCalls {
        #[allow(missing_docs)]
        bridge(bridgeCall),
        #[allow(missing_docs)]
        curvyAggregator(curvyAggregatorCall),
        #[allow(missing_docs)]
        curvyVault(curvyVaultCall),
        #[allow(missing_docs)]
        initialize(initializeCall),
        #[allow(missing_docs)]
        recover(recoverCall),
        #[allow(missing_docs)]
        recovery(recoveryCall),
        #[allow(missing_docs)]
        shield(shieldCall),
    }
    impl PortalCalls {
        /// All the selectors of this enum.
        ///
        /// Note that the selectors might not be in the same order as the variants.
        /// No guarantees are made about the order of the selectors.
        ///
        /// Prefer using `SolInterface` methods instead.
        pub const SELECTORS: &'static [[u8; 4usize]] = &[
            [25u8, 156u8, 84u8, 78u8],
            [63u8, 176u8, 112u8, 39u8],
            [93u8, 194u8, 78u8, 227u8],
            [100u8, 139u8, 247u8, 116u8],
            [118u8, 252u8, 235u8, 118u8],
            [221u8, 206u8, 175u8, 169u8],
            [227u8, 245u8, 197u8, 82u8],
        ];
        /// The names of the variants in the same order as `SELECTORS`.
        pub const VARIANT_NAMES: &'static [&'static str] = &[
            ::core::stringify!(shield),
            ::core::stringify!(curvyVault),
            ::core::stringify!(curvyAggregator),
            ::core::stringify!(recover),
            ::core::stringify!(initialize),
            ::core::stringify!(recovery),
            ::core::stringify!(bridge),
        ];
        /// The signatures in the same order as `SELECTORS`.
        pub const SIGNATURES: &'static [&'static str] = &[
            <shieldCall as alloy_sol_types::SolCall>::SIGNATURE,
            <curvyVaultCall as alloy_sol_types::SolCall>::SIGNATURE,
            <curvyAggregatorCall as alloy_sol_types::SolCall>::SIGNATURE,
            <recoverCall as alloy_sol_types::SolCall>::SIGNATURE,
            <initializeCall as alloy_sol_types::SolCall>::SIGNATURE,
            <recoveryCall as alloy_sol_types::SolCall>::SIGNATURE,
            <bridgeCall as alloy_sol_types::SolCall>::SIGNATURE,
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
    impl alloy_sol_types::SolInterface for PortalCalls {
        const NAME: &'static str = "PortalCalls";
        const MIN_DATA_LENGTH: usize = 0usize;
        const COUNT: usize = 7usize;
        #[inline]
        fn selector(&self) -> [u8; 4] {
            match self {
                Self::bridge(_) => <bridgeCall as alloy_sol_types::SolCall>::SELECTOR,
                Self::curvyAggregator(_) => {
                    <curvyAggregatorCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::curvyVault(_) => {
                    <curvyVaultCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::initialize(_) => {
                    <initializeCall as alloy_sol_types::SolCall>::SELECTOR
                }
                Self::recover(_) => <recoverCall as alloy_sol_types::SolCall>::SELECTOR,
                Self::recovery(_) => <recoveryCall as alloy_sol_types::SolCall>::SELECTOR,
                Self::shield(_) => <shieldCall as alloy_sol_types::SolCall>::SELECTOR,
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
            static DECODE_SHIMS: &[fn(&[u8]) -> alloy_sol_types::Result<PortalCalls>] = &[
                {
                    fn shield(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <shieldCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalCalls::shield)
                    }
                    shield
                },
                {
                    fn curvyVault(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <curvyVaultCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalCalls::curvyVault)
                    }
                    curvyVault
                },
                {
                    fn curvyAggregator(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalCalls> {
                        <curvyAggregatorCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalCalls::curvyAggregator)
                    }
                    curvyAggregator
                },
                {
                    fn recover(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <recoverCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalCalls::recover)
                    }
                    recover
                },
                {
                    fn initialize(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <initializeCall as alloy_sol_types::SolCall>::abi_decode_raw(
                                data,
                            )
                            .map(PortalCalls::initialize)
                    }
                    initialize
                },
                {
                    fn recovery(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <recoveryCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalCalls::recovery)
                    }
                    recovery
                },
                {
                    fn bridge(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <bridgeCall as alloy_sol_types::SolCall>::abi_decode_raw(data)
                            .map(PortalCalls::bridge)
                    }
                    bridge
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
            ) -> alloy_sol_types::Result<PortalCalls>] = &[
                {
                    fn shield(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <shieldCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::shield)
                    }
                    shield
                },
                {
                    fn curvyVault(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <curvyVaultCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::curvyVault)
                    }
                    curvyVault
                },
                {
                    fn curvyAggregator(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalCalls> {
                        <curvyAggregatorCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::curvyAggregator)
                    }
                    curvyAggregator
                },
                {
                    fn recover(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <recoverCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::recover)
                    }
                    recover
                },
                {
                    fn initialize(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <initializeCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::initialize)
                    }
                    initialize
                },
                {
                    fn recovery(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <recoveryCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::recovery)
                    }
                    recovery
                },
                {
                    fn bridge(data: &[u8]) -> alloy_sol_types::Result<PortalCalls> {
                        <bridgeCall as alloy_sol_types::SolCall>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalCalls::bridge)
                    }
                    bridge
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
                Self::bridge(inner) => {
                    <bridgeCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::curvyAggregator(inner) => {
                    <curvyAggregatorCall as alloy_sol_types::SolCall>::abi_encoded_size(
                        inner,
                    )
                }
                Self::curvyVault(inner) => {
                    <curvyVaultCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::initialize(inner) => {
                    <initializeCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::recover(inner) => {
                    <recoverCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::recovery(inner) => {
                    <recoveryCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
                Self::shield(inner) => {
                    <shieldCall as alloy_sol_types::SolCall>::abi_encoded_size(inner)
                }
            }
        }
        #[inline]
        fn abi_encode_raw(&self, out: &mut alloy_sol_types::private::Vec<u8>) {
            match self {
                Self::bridge(inner) => {
                    <bridgeCall as alloy_sol_types::SolCall>::abi_encode_raw(inner, out)
                }
                Self::curvyAggregator(inner) => {
                    <curvyAggregatorCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::curvyVault(inner) => {
                    <curvyVaultCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::initialize(inner) => {
                    <initializeCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::recover(inner) => {
                    <recoverCall as alloy_sol_types::SolCall>::abi_encode_raw(inner, out)
                }
                Self::recovery(inner) => {
                    <recoveryCall as alloy_sol_types::SolCall>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::shield(inner) => {
                    <shieldCall as alloy_sol_types::SolCall>::abi_encode_raw(inner, out)
                }
            }
        }
    }
    ///Container for all the [`Portal`](self) custom errors.
    #[derive(Clone)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub enum PortalErrors {
        #[allow(missing_docs)]
        AlreadyInitialized(AlreadyInitialized),
        #[allow(missing_docs)]
        BridgeCallFailed(BridgeCallFailed),
        #[allow(missing_docs)]
        InsufficientBalanceForLiFiBridging(InsufficientBalanceForLiFiBridging),
        #[allow(missing_docs)]
        InvalidLiFiAddress(InvalidLiFiAddress),
        #[allow(missing_docs)]
        InvalidOwnerHash(InvalidOwnerHash),
        #[allow(missing_docs)]
        InvalidOwnerHashOrExitBridgeData(InvalidOwnerHashOrExitBridgeData),
        #[allow(missing_docs)]
        InvalidRecoveryAddress(InvalidRecoveryAddress),
        #[allow(missing_docs)]
        InvalidSignatureOrTamperedData(InvalidSignatureOrTamperedData),
        #[allow(missing_docs)]
        NoBalance(NoBalance),
        #[allow(missing_docs)]
        SafeERC20FailedOperation(SafeERC20FailedOperation),
    }
    impl PortalErrors {
        /// All the selectors of this enum.
        ///
        /// Note that the selectors might not be in the same order as the variants.
        /// No guarantees are made about the order of the selectors.
        ///
        /// Prefer using `SolInterface` methods instead.
        pub const SELECTORS: &'static [[u8; 4usize]] = &[
            [13u8, 193u8, 73u8, 240u8],
            [55u8, 111u8, 181u8, 90u8],
            [82u8, 116u8, 175u8, 231u8],
            [123u8, 221u8, 122u8, 231u8],
            [164u8, 177u8, 159u8, 152u8],
            [166u8, 20u8, 33u8, 162u8],
            [194u8, 202u8, 162u8, 166u8],
            [203u8, 125u8, 255u8, 0u8],
            [233u8, 0u8, 51u8, 126u8],
            [234u8, 111u8, 92u8, 88u8],
        ];
        /// The names of the variants in the same order as `SELECTORS`.
        pub const VARIANT_NAMES: &'static [&'static str] = &[
            ::core::stringify!(AlreadyInitialized),
            ::core::stringify!(BridgeCallFailed),
            ::core::stringify!(SafeERC20FailedOperation),
            ::core::stringify!(InsufficientBalanceForLiFiBridging),
            ::core::stringify!(InvalidLiFiAddress),
            ::core::stringify!(InvalidRecoveryAddress),
            ::core::stringify!(NoBalance),
            ::core::stringify!(InvalidOwnerHash),
            ::core::stringify!(InvalidSignatureOrTamperedData),
            ::core::stringify!(InvalidOwnerHashOrExitBridgeData),
        ];
        /// The signatures in the same order as `SELECTORS`.
        pub const SIGNATURES: &'static [&'static str] = &[
            <AlreadyInitialized as alloy_sol_types::SolError>::SIGNATURE,
            <BridgeCallFailed as alloy_sol_types::SolError>::SIGNATURE,
            <SafeERC20FailedOperation as alloy_sol_types::SolError>::SIGNATURE,
            <InsufficientBalanceForLiFiBridging as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidLiFiAddress as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidRecoveryAddress as alloy_sol_types::SolError>::SIGNATURE,
            <NoBalance as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidOwnerHash as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidSignatureOrTamperedData as alloy_sol_types::SolError>::SIGNATURE,
            <InvalidOwnerHashOrExitBridgeData as alloy_sol_types::SolError>::SIGNATURE,
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
    impl alloy_sol_types::SolInterface for PortalErrors {
        const NAME: &'static str = "PortalErrors";
        const MIN_DATA_LENGTH: usize = 0usize;
        const COUNT: usize = 10usize;
        #[inline]
        fn selector(&self) -> [u8; 4] {
            match self {
                Self::AlreadyInitialized(_) => {
                    <AlreadyInitialized as alloy_sol_types::SolError>::SELECTOR
                }
                Self::BridgeCallFailed(_) => {
                    <BridgeCallFailed as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InsufficientBalanceForLiFiBridging(_) => {
                    <InsufficientBalanceForLiFiBridging as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidLiFiAddress(_) => {
                    <InvalidLiFiAddress as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidOwnerHash(_) => {
                    <InvalidOwnerHash as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidOwnerHashOrExitBridgeData(_) => {
                    <InvalidOwnerHashOrExitBridgeData as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidRecoveryAddress(_) => {
                    <InvalidRecoveryAddress as alloy_sol_types::SolError>::SELECTOR
                }
                Self::InvalidSignatureOrTamperedData(_) => {
                    <InvalidSignatureOrTamperedData as alloy_sol_types::SolError>::SELECTOR
                }
                Self::NoBalance(_) => <NoBalance as alloy_sol_types::SolError>::SELECTOR,
                Self::SafeERC20FailedOperation(_) => {
                    <SafeERC20FailedOperation as alloy_sol_types::SolError>::SELECTOR
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
            static DECODE_SHIMS: &[fn(&[u8]) -> alloy_sol_types::Result<PortalErrors>] = &[
                {
                    fn AlreadyInitialized(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <AlreadyInitialized as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::AlreadyInitialized)
                    }
                    AlreadyInitialized
                },
                {
                    fn BridgeCallFailed(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <BridgeCallFailed as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::BridgeCallFailed)
                    }
                    BridgeCallFailed
                },
                {
                    fn SafeERC20FailedOperation(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <SafeERC20FailedOperation as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::SafeERC20FailedOperation)
                    }
                    SafeERC20FailedOperation
                },
                {
                    fn InsufficientBalanceForLiFiBridging(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InsufficientBalanceForLiFiBridging as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::InsufficientBalanceForLiFiBridging)
                    }
                    InsufficientBalanceForLiFiBridging
                },
                {
                    fn InvalidLiFiAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidLiFiAddress as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::InvalidLiFiAddress)
                    }
                    InvalidLiFiAddress
                },
                {
                    fn InvalidRecoveryAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidRecoveryAddress as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::InvalidRecoveryAddress)
                    }
                    InvalidRecoveryAddress
                },
                {
                    fn NoBalance(data: &[u8]) -> alloy_sol_types::Result<PortalErrors> {
                        <NoBalance as alloy_sol_types::SolError>::abi_decode_raw(data)
                            .map(PortalErrors::NoBalance)
                    }
                    NoBalance
                },
                {
                    fn InvalidOwnerHash(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidOwnerHash as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::InvalidOwnerHash)
                    }
                    InvalidOwnerHash
                },
                {
                    fn InvalidSignatureOrTamperedData(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidSignatureOrTamperedData as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::InvalidSignatureOrTamperedData)
                    }
                    InvalidSignatureOrTamperedData
                },
                {
                    fn InvalidOwnerHashOrExitBridgeData(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidOwnerHashOrExitBridgeData as alloy_sol_types::SolError>::abi_decode_raw(
                                data,
                            )
                            .map(PortalErrors::InvalidOwnerHashOrExitBridgeData)
                    }
                    InvalidOwnerHashOrExitBridgeData
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
            ) -> alloy_sol_types::Result<PortalErrors>] = &[
                {
                    fn AlreadyInitialized(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <AlreadyInitialized as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::AlreadyInitialized)
                    }
                    AlreadyInitialized
                },
                {
                    fn BridgeCallFailed(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <BridgeCallFailed as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::BridgeCallFailed)
                    }
                    BridgeCallFailed
                },
                {
                    fn SafeERC20FailedOperation(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <SafeERC20FailedOperation as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::SafeERC20FailedOperation)
                    }
                    SafeERC20FailedOperation
                },
                {
                    fn InsufficientBalanceForLiFiBridging(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InsufficientBalanceForLiFiBridging as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::InsufficientBalanceForLiFiBridging)
                    }
                    InsufficientBalanceForLiFiBridging
                },
                {
                    fn InvalidLiFiAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidLiFiAddress as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::InvalidLiFiAddress)
                    }
                    InvalidLiFiAddress
                },
                {
                    fn InvalidRecoveryAddress(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidRecoveryAddress as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::InvalidRecoveryAddress)
                    }
                    InvalidRecoveryAddress
                },
                {
                    fn NoBalance(data: &[u8]) -> alloy_sol_types::Result<PortalErrors> {
                        <NoBalance as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::NoBalance)
                    }
                    NoBalance
                },
                {
                    fn InvalidOwnerHash(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidOwnerHash as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::InvalidOwnerHash)
                    }
                    InvalidOwnerHash
                },
                {
                    fn InvalidSignatureOrTamperedData(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidSignatureOrTamperedData as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::InvalidSignatureOrTamperedData)
                    }
                    InvalidSignatureOrTamperedData
                },
                {
                    fn InvalidOwnerHashOrExitBridgeData(
                        data: &[u8],
                    ) -> alloy_sol_types::Result<PortalErrors> {
                        <InvalidOwnerHashOrExitBridgeData as alloy_sol_types::SolError>::abi_decode_raw_validate(
                                data,
                            )
                            .map(PortalErrors::InvalidOwnerHashOrExitBridgeData)
                    }
                    InvalidOwnerHashOrExitBridgeData
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
                Self::AlreadyInitialized(inner) => {
                    <AlreadyInitialized as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::BridgeCallFailed(inner) => {
                    <BridgeCallFailed as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InsufficientBalanceForLiFiBridging(inner) => {
                    <InsufficientBalanceForLiFiBridging as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidLiFiAddress(inner) => {
                    <InvalidLiFiAddress as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidOwnerHash(inner) => {
                    <InvalidOwnerHash as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidOwnerHashOrExitBridgeData(inner) => {
                    <InvalidOwnerHashOrExitBridgeData as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidRecoveryAddress(inner) => {
                    <InvalidRecoveryAddress as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::InvalidSignatureOrTamperedData(inner) => {
                    <InvalidSignatureOrTamperedData as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
                Self::NoBalance(inner) => {
                    <NoBalance as alloy_sol_types::SolError>::abi_encoded_size(inner)
                }
                Self::SafeERC20FailedOperation(inner) => {
                    <SafeERC20FailedOperation as alloy_sol_types::SolError>::abi_encoded_size(
                        inner,
                    )
                }
            }
        }
        #[inline]
        fn abi_encode_raw(&self, out: &mut alloy_sol_types::private::Vec<u8>) {
            match self {
                Self::AlreadyInitialized(inner) => {
                    <AlreadyInitialized as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::BridgeCallFailed(inner) => {
                    <BridgeCallFailed as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InsufficientBalanceForLiFiBridging(inner) => {
                    <InsufficientBalanceForLiFiBridging as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidLiFiAddress(inner) => {
                    <InvalidLiFiAddress as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidOwnerHash(inner) => {
                    <InvalidOwnerHash as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidOwnerHashOrExitBridgeData(inner) => {
                    <InvalidOwnerHashOrExitBridgeData as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidRecoveryAddress(inner) => {
                    <InvalidRecoveryAddress as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::InvalidSignatureOrTamperedData(inner) => {
                    <InvalidSignatureOrTamperedData as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
                Self::NoBalance(inner) => {
                    <NoBalance as alloy_sol_types::SolError>::abi_encode_raw(inner, out)
                }
                Self::SafeERC20FailedOperation(inner) => {
                    <SafeERC20FailedOperation as alloy_sol_types::SolError>::abi_encode_raw(
                        inner,
                        out,
                    )
                }
            }
        }
    }
    use alloy::contract as alloy_contract;
    /**Creates a new wrapper around an on-chain [`Portal`](self) contract instance.

See the [wrapper's documentation](`PortalInstance`) for more details.*/
    #[inline]
    pub const fn new<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    >(
        address: alloy_sol_types::private::Address,
        __provider: P,
    ) -> PortalInstance<P, N> {
        PortalInstance::<P, N>::new(address, __provider)
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
    ) -> impl ::core::future::Future<
        Output = alloy_contract::Result<PortalInstance<P, N>>,
    > {
        PortalInstance::<P, N>::deploy(__provider)
    }
    /**Creates a `RawCallBuilder` for deploying this contract using the given `provider`
and constructor arguments, if any.

This is a simple wrapper around creating a `RawCallBuilder` with the data set to
the bytecode concatenated with the constructor's ABI-encoded arguments.*/
    #[inline]
    pub fn deploy_builder<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    >(__provider: P) -> alloy_contract::RawCallBuilder<P, N> {
        PortalInstance::<P, N>::deploy_builder(__provider)
    }
    /**A [`Portal`](self) instance.

Contains type-safe methods for interacting with an on-chain instance of the
[`Portal`](self) contract located at a given `address`, using a given
provider `P`.

If the contract bytecode is available (see the [`sol!`](alloy_sol_types::sol!)
documentation on how to provide it), the `deploy` and `deploy_builder` methods can
be used to deploy a new instance of the contract.

See the [module-level documentation](self) for all the available methods.*/
    #[derive(Clone)]
    pub struct PortalInstance<P, N = alloy_contract::private::Ethereum> {
        address: alloy_sol_types::private::Address,
        provider: P,
        _network: ::core::marker::PhantomData<N>,
    }
    #[automatically_derived]
    impl<P, N> ::core::fmt::Debug for PortalInstance<P, N> {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple("PortalInstance").field(&self.address).finish()
        }
    }
    /// Instantiation and getters/setters.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > PortalInstance<P, N> {
        /**Creates a new wrapper around an on-chain [`Portal`](self) contract instance.

See the [wrapper's documentation](`PortalInstance`) for more details.*/
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
        ) -> alloy_contract::Result<PortalInstance<P, N>> {
            let call_builder = Self::deploy_builder(__provider);
            let contract_address = call_builder.deploy().await?;
            Ok(Self::new(contract_address, call_builder.provider))
        }
        /**Creates a `RawCallBuilder` for deploying this contract using the given `provider`
and constructor arguments, if any.

This is a simple wrapper around creating a `RawCallBuilder` with the data set to
the bytecode concatenated with the constructor's ABI-encoded arguments.*/
        #[inline]
        pub fn deploy_builder(__provider: P) -> alloy_contract::RawCallBuilder<P, N> {
            alloy_contract::RawCallBuilder::new_raw_deploy(
                __provider,
                ::core::clone::Clone::clone(&BYTECODE),
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
    impl<P: ::core::clone::Clone, N> PortalInstance<&P, N> {
        /// Clones the provider and returns a new instance with the cloned provider.
        #[inline]
        pub fn with_cloned_provider(self) -> PortalInstance<P, N> {
            PortalInstance {
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
    > PortalInstance<P, N> {
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
        ///Creates a new call builder for the [`bridge`] function.
        pub fn bridge(
            &self,
            lifiDiamondAddress: alloy::sol_types::private::Address,
            bridgeData: alloy::sol_types::private::Bytes,
            amount: alloy::sol_types::private::primitives::aliases::U256,
            currency: alloy::sol_types::private::Address,
            gasFee: alloy::sol_types::private::primitives::aliases::U256,
        ) -> alloy_contract::SolCallBuilder<&P, bridgeCall, N> {
            self.call_builder(
                &bridgeCall {
                    lifiDiamondAddress,
                    bridgeData,
                    amount,
                    currency,
                    gasFee,
                },
            )
        }
        ///Creates a new call builder for the [`curvyAggregator`] function.
        pub fn curvyAggregator(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, curvyAggregatorCall, N> {
            self.call_builder(&curvyAggregatorCall)
        }
        ///Creates a new call builder for the [`curvyVault`] function.
        pub fn curvyVault(
            &self,
        ) -> alloy_contract::SolCallBuilder<&P, curvyVaultCall, N> {
            self.call_builder(&curvyVaultCall)
        }
        ///Creates a new call builder for the [`initialize`] function.
        pub fn initialize(
            &self,
            ownerHash: alloy::sol_types::private::primitives::aliases::U256,
            exitAddress: alloy::sol_types::private::Address,
            exitChainId: alloy::sol_types::private::primitives::aliases::U256,
            _recovery: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, initializeCall, N> {
            self.call_builder(
                &initializeCall {
                    ownerHash,
                    exitAddress,
                    exitChainId,
                    _recovery,
                },
            )
        }
        ///Creates a new call builder for the [`recover`] function.
        pub fn recover(
            &self,
            tokenAddress: alloy::sol_types::private::Address,
            to: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, recoverCall, N> {
            self.call_builder(&recoverCall { tokenAddress, to })
        }
        ///Creates a new call builder for the [`recovery`] function.
        pub fn recovery(&self) -> alloy_contract::SolCallBuilder<&P, recoveryCall, N> {
            self.call_builder(&recoveryCall)
        }
        ///Creates a new call builder for the [`shield`] function.
        pub fn shield(
            &self,
            note: <CurvyTypes::Note as alloy::sol_types::SolType>::RustType,
            curvyAggregatorAlphaProxyAddress: alloy::sol_types::private::Address,
            curvyVaultProxyAddress: alloy::sol_types::private::Address,
        ) -> alloy_contract::SolCallBuilder<&P, shieldCall, N> {
            self.call_builder(
                &shieldCall {
                    note,
                    curvyAggregatorAlphaProxyAddress,
                    curvyVaultProxyAddress,
                },
            )
        }
    }
    /// Event filters.
    impl<
        P: alloy_contract::private::Provider<N>,
        N: alloy_contract::private::Network,
    > PortalInstance<P, N> {
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
