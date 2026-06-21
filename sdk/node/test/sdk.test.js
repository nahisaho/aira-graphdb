import test from "node:test";
import assert from "node:assert/strict";

import {
  createHandshakeRequest,
  loadErrorCodeContract,
  loadTypeMapContract,
  mapKnownError
} from "../src/index.js";

test("loads typemap contract", () => {
  const contract = loadTypeMapContract();
  assert.equal(contract.spec_id, "AGDB-TYPEMAP-P0@1.0.0");
});

test("creates handshake request", () => {
  const payload = createHandshakeRequest("protocol-p0@1.0.0", "canonical-types@1.0.0");
  assert.equal(payload.protocol_version, "protocol-p0@1.0.0");
});

test("maps known and unknown errors", () => {
  const known = mapKnownError("AUTH_FAILED", "fail");
  assert.equal(known.code, "AUTH_FAILED");
  const unknown = mapKnownError("NOT_REAL", "fail");
  assert.equal(unknown.code, "UNSUPPORTED_FEATURE");

  const errorSpec = loadErrorCodeContract();
  assert.equal(errorSpec.spec_id, "AGDB-ERROR-CODES@1.0.0");
});
