// A trusted writer's markup, or a saved briefing, as it reads: compiled in the closed world, so
// it reaches neither `html` nor `plugin` nor a file.
#import "/schema/Markup.typ" as markup

#eval(markup.source(json("/state.json")), mode: "markup")
