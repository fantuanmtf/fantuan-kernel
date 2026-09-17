#!/usr/bin/env bash
# gen_auth_fixture.sh — regenerate the EFI authenticated-variable fixture
# embedded as AUTH_BLOB in tools/mkdisk_fat.py (M8.1b). Development-time
# only: requires openssl. The script prints a Python literal; paste it over
# the AUTH_BLOB block in mkdisk_fat.py. Each run generates a fresh key, so
# the bytes differ between runs — the blob checked into the tree is the one
# generated when M8.1b landed.
#
# Layout (UEFI 2.10 §8.2.3, confirmed against EDK2 AuthService.c):
#   EFI_VARIABLE_AUTHENTICATION_2:
#     EFI_TIME (16 bytes, Pad/Nanosecond/TimeZone/Daylight/Pad2 = 0)
#     WIN_CERTIFICATE_UEFI_GUID:
#       dwLength = 24 + len(pkcs7), wRevision = 0x0200,
#       wCertificateType = WIN_CERT_TYPE_EFI_GUID (0x0EF1),
#       CertType = gEfiCertPkcs7Guid, CertData = PKCS#7 SignedData
#   followed by the new variable value.
set -euo pipefail

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$TMP/key.pem" 2>/dev/null
openssl req -x509 -new -key "$TMP/key.pem" -subj "/CN=Fantuan Test KEK" -days 3650 \
    -out "$TMP/cert.pem" 2>/dev/null
printf 'fantuan auth payload (variable data)' > "$TMP/payload.bin"
# -noattr: EFI does not use authenticated attributes.
openssl smime -sign -binary -noattr -nodetach -outform DER -md sha256 \
    -in "$TMP/payload.bin" -signer "$TMP/cert.pem" -inkey "$TMP/key.pem" \
    -out "$TMP/signed.p7" 2>/dev/null

python3 - "$TMP" <<'PY'
import base64, struct, sys

tmp = sys.argv[1]
p7 = open(f"{tmp}/signed.p7", "rb").read()
payload = open(f"{tmp}/payload.bin", "rb").read()
ts = bytes(16)  # all-zero EFI_TIME fields EDK2 requires to be zero
wincert = struct.pack("<IHH", 24 + len(p7), 0x0200, 0x0EF1)
guid = bytes.fromhex("9dd2af4adf68ee498aa9347d375665a7")  # gEfiCertPkcs7Guid
blob = ts + wincert + guid + p7 + payload
b64 = base64.b64encode(blob).decode()
print("AUTH_BLOB = base64.b64decode(")
for i in range(0, len(b64), 96):
    print('    b"%s"' % b64[i:i + 96])
print(")")
PY
