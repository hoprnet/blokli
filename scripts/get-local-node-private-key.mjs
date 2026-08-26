#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const ADDRESS_PATTERN = /^0x[0-9a-fA-F]{40}$/;
const PRIVATE_KEY_PATTERN = /^[0-9a-fA-F]{64}$/;

function usage() {
  console.error(`Usage: ${path.basename(process.argv[1])} <ethereum-address>

Environment variables:
  HOPR_IDENTITY_DIR       Identity directory (default: /tmp/hopr-nodes)
  HOPR_IDENTITY_PASSWORD  Identity password (default: password)
  HOPR_CHAIN_CONTAINER    Container providing cast (default: hopr-chain)`);
}

function decryptChainKey(identityPath, password) {
  const keystore = JSON.parse(fs.readFileSync(identityPath, "utf8"));
  const cryptoJson = keystore.crypto;

  if (cryptoJson?.kdf !== "scrypt" || cryptoJson?.cipher !== "aes-128-ctr") {
    throw new Error("unsupported identity keystore format");
  }

  const params = cryptoJson.kdfparams;
  const derivedKey = crypto.scryptSync(
    password,
    Buffer.from(params.salt, "hex"),
    params.dklen,
    {
      N: params.n,
      r: params.r,
      p: params.p,
      maxmem: 128 * 1024 * 1024,
    },
  );
  const decipher = crypto.createDecipheriv(
    "aes-128-ctr",
    derivedKey.subarray(0, 16),
    Buffer.from(cryptoJson.cipherparams.iv, "hex"),
  );
  const plaintext = Buffer.concat([
    decipher.update(Buffer.from(cryptoJson.ciphertext, "hex")),
    decipher.final(),
  ]);
  const privateKeys = JSON.parse(plaintext.toString("utf8"));
  const encodedChainKey = Buffer.from(privateKeys.chain_key);
  const chainKey =
    encodedChainKey.length === 32
      ? encodedChainKey.toString("hex")
      : encodedChainKey.toString("utf8");

  if (!PRIVATE_KEY_PATTERN.test(chainKey)) {
    throw new Error("identity contains an invalid chain key");
  }

  return `0x${chainKey.toLowerCase()}`;
}

function addressForPrivateKey(container, privateKey) {
  const result = spawnSync(
    "docker",
    [
      "exec",
      container,
      "cast",
      "wallet",
      "address",
      "--private-key",
      privateKey,
    ],
    { encoding: "utf8" },
  );

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      result.stderr.trim() || "cast failed in the chain container",
    );
  }

  return result.stdout.trim().toLowerCase();
}

const requestedAddress = process.argv[2];
if (!requestedAddress || !ADDRESS_PATTERN.test(requestedAddress)) {
  usage();
  process.exit(2);
}

const identityDir = process.env.HOPR_IDENTITY_DIR ?? "/tmp/hopr-nodes";
const password = process.env.HOPR_IDENTITY_PASSWORD ?? "password";
const container = process.env.HOPR_CHAIN_CONTAINER ?? "hopr-chain";
const identityFiles = fs
  .readdirSync(identityDir)
  .filter((name) => /^node_id_\d+\.id$/.test(name))
  .sort((left, right) =>
    left.localeCompare(right, undefined, { numeric: true }),
  );

if (identityFiles.length === 0) {
  console.error(`No node identity files found in ${identityDir}`);
  process.exit(1);
}

for (const identityFile of identityFiles) {
  const identityPath = path.join(identityDir, identityFile);

  try {
    const privateKey = decryptChainKey(identityPath, password);
    const address = addressForPrivateKey(container, privateKey);

    if (address === requestedAddress.toLowerCase()) {
      console.log(privateKey);
      process.exit(0);
    }
  } catch (error) {
    console.error(`Unable to inspect ${identityPath}: ${error.message}`);
  }
}

console.error(`No local node identity matches ${requestedAddress}`);
process.exit(1);
