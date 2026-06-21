import json
from pathlib import Path


CONTRACT_ROOT = Path(__file__).resolve().parents[3] / "spec" / "contracts"


def load_typemap_contract() -> dict:
    with open(CONTRACT_ROOT / "agdb-typemap-p0.v1.0.0.json", "r", encoding="utf-8") as f:
        return json.load(f)


def load_error_contract() -> dict:
    with open(CONTRACT_ROOT / "agdb-error-codes.v1.0.0.json", "r", encoding="utf-8") as f:
        return json.load(f)


def create_handshake_request(protocol_version: str, canonical_type_system_version: str) -> dict:
    return {
        "protocol_version": protocol_version,
        "canonical_type_system_version": canonical_type_system_version,
    }


def map_known_error(code: str, message: str) -> dict:
    contract = load_error_contract()
    known = any(item["code"] == code for item in contract["codes"])
    return {"code": code if known else "UNSUPPORTED_FEATURE", "message": message}
