#!/bin/bash

API_URL="${API_URL}"
INDEX_FILE="./docs/index.html"

sed -i "s/{{API_URL}}/${API_URL}/g" "$INDEX_FILE"
