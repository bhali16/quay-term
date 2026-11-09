# quay-term shell integration for zsh: marks prompts and commands with OSC 133.
[[ -o interactive ]] || return 0
(( ${+__quay_integrated} )) && return 0
typeset -g __quay_integrated=1
# D ends the last command; 7 reports the directory the next one runs in; A starts the prompt.
__quay_precmd() { local e=$?; printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' "$e" "${HOST-}" "$PWD"; }
__quay_preexec() { printf '\e]133;C\a'; }
# Appended last so prompt themes that rebuild PS1 on every prompt keep the end-of-prompt mark.
__quay_prompt_end() { [[ $PS1 == *133?B* ]] || PS1=$PS1$'%{\e]133;B\a%}'; }
precmd_functions=(__quay_precmd $precmd_functions __quay_prompt_end)
preexec_functions+=(__quay_preexec)
