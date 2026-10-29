#!/usr/bin/env bash
# Creates the self-signed "dictation-app Dev" code-signing certificate in the
# login keychain. Run once per Mac (and in the test VM). Tauri signs every
# build with it (bundle.macOS.signingIdentity), so macOS keeps Accessibility
# and Microphone permission across rebuilds (docs/ENGINEERING.md "Signing").
#
# macOS asks for your login password once, to trust the certificate for code
# signing. Safe to re-run: it does nothing if the identity already exists.
set -euo pipefail

name="dictation-app Dev"
keychain="$HOME/Library/Keychains/login.keychain-db"
# The system LibreSSL writes PKCS#12 files that `security import` accepts;
# Homebrew's OpenSSL 3 needs extra flags for that.
openssl=/usr/bin/openssl

if [ "$(uname -s)" != Darwin ]; then
  echo "make-dev-cert: macOS only" >&2
  exit 1
fi

if security find-identity -p codesigning "$keychain" | grep -qF "\"$name\""; then
  echo "make-dev-cert: \"$name\" is already in the login keychain"
  security find-identity -p codesigning "$keychain" | grep -F "\"$name\""
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Only protects the PKCS#12 file between export and import; never stored.
password=$("$openssl" rand -hex 16)

cat >"$work/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no

[dn]
CN = $name

[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF

"$openssl" req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 \
  -config "$work/cert.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
"$openssl" pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" \
  -name "$name" -passout "pass:$password" -out "$work/cert.p12"

# -T lets codesign use the key without a keychain prompt on every build.
security import "$work/cert.p12" -k "$keychain" -P "$password" -T /usr/bin/codesign
# Trust it for code signing only, for this user only.
security add-trusted-cert -r trustRoot -p codeSign -k "$keychain" "$work/cert.pem"

echo "make-dev-cert: created \"$name\":"
security find-identity -v -p codesigning "$keychain" | grep -F "\"$name\""
