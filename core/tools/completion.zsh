#compdef dtech-aidlc

_dtech-aidlc() {
  local -a scopes harnesses models agents results
  scopes=(solo team enterprise)
  harnesses=(claude deepseek)
  models=(opus sonnet haiku fable)
  agents=(
    brief-analyst context-risk-analyst
    solution-explorer decision-facilitator
    implementation-planner builder-verifier
    release-coordinator operations-handoff
    outcome-curator knowledge-steward
    record-keeper knowledge-archivist
  )
  results=(complete blocked skipped)

  _arguments -C \
    '--project[Project directory]:directory:_directories' \
    '--help[Show help]' \
    '--version[Show version]' \
    '1:command:->command' \
    '*::arg:->args'

  case $state in
    command)
      local -a commands
      commands=(
        'help:Show help'
        'version:Show version'
        'onboard:Initialize and detect the project'
        'init:Alias of onboard'
        'scope:Show stages in a scope'
        'status:Show the active intent'
        'doctor:Validate project setup'
        'orchestrate:Advance or report a stage'
        'config:Configure agent models'
        'repos:Manage dependency repositories'
        'deps:Manage workflow dependency edges'
        'intent:List, pause, resume, or abandon intents'
        'knowledge:Manage project knowledge and sources'
        'completion:Print shell completion'
      )
      _describe 'command' commands
      ;;
    args)
      case ${words[1]} in
        init|onboard)
          _arguments \
            '--harness[Coding harness]:harness:(claude deepseek)' \
            '--scope[Workflow scope]:scope:(solo team enterprise)' \
            '--rescan[Re-scan an initialized project]' \
            '--docs[Documentation path]:path:_files' \
            '--no-detect[Skip automatic documentation detection]'
          ;;
        scope)
          _arguments '1:scope:(solo team enterprise)'
          ;;
        status)
          _arguments '--intent[Intent id]:intent:'
          ;;
        knowledge)
          _arguments -C '1:action:(init sources list show check index)' '*::arg:->knowledgerest'
          case $state in
            knowledgerest)
              case ${words[1]} in
                sources) _arguments '1:sub:(list add remove)' '--kind[Source kind]:kind:(project docs requirements architecture adr api-spec runbook conventions readme)' '--description[Description]:description:' '2:id:' '3:path:' ;;
                show) _arguments '1:id:' ;;
              esac
              ;;
          esac
          ;;
        intent)
          _arguments -C '1:action:(list pause resume abandon)' '*::arg:->intentrest'
          case $state in
            intentrest)
              case ${words[1]} in
                pause) _arguments '--detail[Reason]:detail:' ;;
                resume) _arguments '1:id:' ;;
                abandon) _arguments '--detail[Reason]:detail:' '1:id:' ;;
              esac
              ;;
          esac
          ;;
        orchestrate)
          _arguments -C '1:action:->action' '*::arg:->rest'
          case $state in
            action) _values 'action' next report ;;
            rest)
              case ${words[1]} in
                next) _arguments '--intent[Intent text]:intent:' '--scope[Workflow scope]:scope:(solo team enterprise)' ;;
                report)
                  _arguments \
                    '--stage[Stage identifier]:stage:' \
                    '--result[Stage result]:result:(complete blocked skipped)' \
                    '--approval[Phase approval]:approval:(approved)' \
                    '--detail[Result detail]:detail:'
                  ;;
              esac
              ;;
          esac
          ;;
        config)
          _arguments -C '1:section:(agents)' '2:action:->action' '*::arg:->rest'
          case ${words[2]} in
            set) _arguments '1:agent:(brief-analyst context-risk-analyst solution-explorer decision-facilitator implementation-planner builder-verifier release-coordinator operations-handoff outcome-curator knowledge-steward record-keeper knowledge-archivist)' '2:model:(opus sonnet haiku fable)' ;;
            reset) _arguments '1:agent:(brief-analyst context-risk-analyst solution-explorer decision-facilitator implementation-planner builder-verifier release-coordinator operations-handoff outcome-curator knowledge-steward record-keeper knowledge-archivist)' ;;
          esac
          ;;
        repos)
          _arguments -C '1:action:->action' '*::arg:->rest'
          case $state in
            action) _values 'action' add list pull remove ;;
            rest)
              case ${words[1]} in
                add) _arguments '--ref[Git ref]:ref:' '1:name:' '2:url:_urls' ;;
                pull) _arguments '--file[Dependency JSON file]:file:_files' '1:name:' ;;
                remove) _arguments '1:name:' ;;
              esac
              ;;
          esac
          ;;
        deps)
          _arguments '1:action:(add list pull)' '2:id:' '3:dependency:'
          ;;
        completion)
          _arguments '1:shell:(zsh)'
          ;;
      esac
      ;;
  esac
}

_dtech-aidlc "$@"
