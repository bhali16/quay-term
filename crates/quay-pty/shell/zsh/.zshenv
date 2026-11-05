# quay-term shell integration shim. Reads your real .zshenv, then points zsh back at this
# directory so the later startup files (.zprofile, .zshrc) also go through the shim.
_q_shim=$ZDOTDIR
_q_home=${QUAY_ORIG_ZDOTDIR:-$HOME}
ZDOTDIR=$_q_home
[[ -r $_q_home/.zshenv ]] && source "$_q_home/.zshenv"
# A .zshenv that moves ZDOTDIR is honoured.
if [[ $ZDOTDIR != "$_q_home" ]]; then
  export QUAY_ORIG_ZDOTDIR=$ZDOTDIR
fi
ZDOTDIR=$_q_shim
unset _q_shim _q_home
