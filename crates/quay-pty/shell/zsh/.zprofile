# quay-term shell integration shim: loads your real .zprofile.
[[ -r ${QUAY_ORIG_ZDOTDIR:-$HOME}/.zprofile ]] && source "${QUAY_ORIG_ZDOTDIR:-$HOME}/.zprofile"
