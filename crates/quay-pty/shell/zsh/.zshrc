# quay-term shell integration shim: loads your real .zshrc, restores ZDOTDIR, then adds the hooks.
_q_home=${QUAY_ORIG_ZDOTDIR:-$HOME}
ZDOTDIR=$_q_home
[[ -r $_q_home/.zshrc ]] && source "$_q_home/.zshrc"
if [[ -n $QUAY_ORIG_ZDOTDIR ]]; then
  ZDOTDIR=$QUAY_ORIG_ZDOTDIR
else
  unset ZDOTDIR
fi
unset QUAY_ORIG_ZDOTDIR _q_home
[[ -r $QUAY_SHELL_INTEGRATION_DIR/quay.zsh ]] && source "$QUAY_SHELL_INTEGRATION_DIR/quay.zsh"
unset QUAY_SHELL_INTEGRATION_DIR
