#!/bin/bash

# Script to guide user through generating Tauri signing keys

echo "========================================================"
echo "   Tauri Updater Signing Key Generation Guide"
echo "========================================================"
echo ""
echo "This script will help you generate the keys required for"
echo "Tauri's auto-updater to work securey."
echo ""
echo "1. We will run 'tauri signer generate -w ./crates/app/src-tauri/tauri.conf.json'"
echo "2. You will be asked to enter a password."
echo "3. The public key will be automatically added to tauri.conf.json"
echo "4. The private key will be saved to a file."
echo ""
echo "IMPORTANT: You must keep the private key and password SAFE."
echo "You will need to add them to your GitHub Repository Secrets:"
echo "   - TAURI_PRIVATE_KEY: The content of the private key file"
echo "   - TAURI_KEY_PASSWORD: The password you set"
echo ""
read -p "Press [Enter] to continue..."

# Ensure we are in the root
if [ ! -d "crates/app" ]; then
    echo "Error: crates/app directory not found. Please run this from the project root."
    exit 1
fi

# Run tauri signer generate
# We use npx to run tauri CLI from the app directory context
echo "Running tauri signer generate..."
if command -v cargo-tauri &> /dev/null; then
    cargo tauri signer generate -w ./crates/app/src-tauri/tauri.conf.json
else
    # Fallback to npm if cargo-tauri is not globally installed
    cd crates/app
    npm run tauri signer generate -- -w src-tauri/tauri.conf.json
fi

echo ""
echo "========================================================"
echo "   Done!"
echo "========================================================"
echo "Now, please:"
echo "1. Open the generated private key file (check output above for path)"
echo "2. Go to GitHub -> Settings -> Secrets and variables -> Actions"
echo "3. Add TAURI_PRIVATE_KEY (content of the .key file)"
echo "4. Add TAURI_KEY_PASSWORD (the password you used)"
echo "========================================================"
