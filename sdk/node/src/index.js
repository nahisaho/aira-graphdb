import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const contractRoot = path.resolve(__dirname, "../../../spec/contracts");

export function loadTypeMapContract() {
  const raw = fs.readFileSync(path.join(contractRoot, "agdb-typemap-p0.v1.0.0.json"), "utf8");
  return JSON.parse(raw);
}

export function loadErrorCodeContract() {
  const raw = fs.readFileSync(path.join(contractRoot, "agdb-error-codes.v1.0.0.json"), "utf8");
  return JSON.parse(raw);
}

export function createHandshakeRequest(protocolVersion, canonicalTypeSystemVersion) {
  return {
    protocol_version: protocolVersion,
    canonical_type_system_version: canonicalTypeSystemVersion
  };
}

export function mapKnownError(code, message) {
  const spec = loadErrorCodeContract();
  const known = spec.codes.some((item) => item.code === code);
  return {
    code: known ? code : "UNSUPPORTED_FEATURE",
    message
  };
}
