#!/bin/bash
# Extract certificate fingerprint from lan-mouse certificate file

set -e

# Expand ~ to home directory
CERT_PATH="${1:-$HOME/.config/lan-mouse/lan-mouse.pem}"

if [ ! -f "$CERT_PATH" ]; then
    echo "Error: Certificate file not found: $CERT_PATH"
    exit 1
fi

echo "Extracting fingerprint from: $CERT_PATH"
echo ""

# Extract the certificate in DER format and calculate SHA256 fingerprint
FINGERPRINT=$(openssl x509 -in "$CERT_PATH" -outform DER | openssl dgst -sha256 -binary | xxd -p -c 32 | tr -d ' \n' | tr '[:lower:]' '[:upper:]' | sed 's/\(..\)/\1:/g' | sed 's/:$//')

echo "Certificate fingerprint:"
echo "$FINGERPRINT"
echo ""

# Also show the fingerprint in lowercase (as used in lan-mouse config)
FINGERPRINT_LOWER=$(echo "$FINGERPRINT" | tr '[:upper:]' '[:lower:]')
echo "Certificate fingerprint (lowercase, for config):"
echo "$FINGERPRINT_LOWER"
echo ""

echo "Add this to your authorized_fingerprints section in ~/.config/lan-mouse/config.toml:"
echo '[authorized_fingerprints]'
echo "\"$FINGERPRINT_LOWER\" = \"description\""
