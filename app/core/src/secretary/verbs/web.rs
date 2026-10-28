use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::store::Proposal;
use crate::secretary::verbs::{Reasoned, propose};
use crate::secretary::{self, Call, SecretarySurface, taint};
use crate::verb::Verb;
use crate::web::{self, AllowRule, SearchQuery, WebError, WebRequest};

/// Read the web: an allowed host at once, any other on a tap.
pub struct Web;

#[derive(Deserialize, JsonSchema)]
pub struct WebArgs {
    search: Option<String>,
    url: Option<String>,
    always: Option<String>,
    #[serde(flatten)]
    reasoned: Reasoned,
}

impl Verb for Web {
    type Surface = SecretarySurface;
    type Args = WebArgs;
    type Answer = Value;
    const NAME: &'static str = "web";
    const DESCRIPTION: &'static str = "READ THE WEB: `search` for words, or `url` to read one page (give \
        exactly one). A request on a host the owner has allowed runs at once and \
        answers the page as text. Anything else is ASKED: it is stored as a \
        request they see whole on a card and tap or dismiss, and the page \
        arrives in this conversation if they tap. So never put anything private \
        in a URL or a query, since the URL is what leaves the box; search for \
        the public question, not their case.\n\n\
        In a round that has already read mail, a todo, an attachment, a \
        document, a repository or an old answer, every request is ASKED, an \
        allowed host's too.\n\n\
        `always` is an optional rule to offer as \"Always allow\" on that card: \
        `host.example`, `*.example.org` for a domain and its subdomains, either \
        followed by `/path` to narrow it. Offer one only for a site they will \
        plainly want read again (official docs, a government site); they decide. \
        Redirects are not followed: a redirect answers where it pointed, which \
        is a new request. `why` and `sources` are required as for proposals.";

    fn run(call: &Call<'_>, args: WebArgs) -> Result<Value, Refusal> {
        let reasons = args.reasoned.reasons()?;
        let request = match (args.search, args.url) {
            (Some(words), None) => WebRequest::Search {
                query: SearchQuery::try_from(words)
                    .map_err(|failed| Refusal::bad_request(format!("search: {failed}")))?,
            },
            (None, Some(url)) => WebRequest::Fetch {
                url: url
                    .parse()
                    .map_err(|failed: WebError| Refusal::bad_request(format!("url: {failed}")))?,
            },
            _ => return Err(Refusal::bad_request("give exactly one of search or url")),
        };
        let rule = args
            .always
            .map(|text| {
                text.parse::<AllowRule>()
                    .map_err(|failed| Refusal::bad_request(format!("always: {failed}")))
            })
            .transpose()?;
        // A round that has read mail may be asking on that mail's behalf, and a fetch carries its
        // URL off the box, so there an allowed host is asked too, as `remember` asks.
        let read_outside = taint::tainted(&secretary::read(
            call.caps.conversations,
            call.conversation,
        )?);
        if !read_outside && web::allowed(&call.caps.web.allow_rules(), &request) {
            return Ok(match call.caps.web.perform(&request) {
                Ok(answer) => json!({
                    "url": answer.url,
                    "status": answer.status,
                    "text": answer.text,
                }),
                Err(failed) => json!({ "failed": failed.to_string() }),
            });
        }
        let sentence = request.sentence();
        let index = propose(
            call.caps,
            call.chains,
            call.conversation,
            Proposal::Act(Act::Web { request, rule }),
            &reasons,
            None,
            None,
        )?;
        Ok(json!({
            "asked": index,
            "request": sentence,
            "next": if read_outside {
                "this round has read something that is not the owner's own words, so even an \
                 allowed host is asked; the page arrives here as a turn if they tap it"
            } else {
                "no rule allows this host, so the owner was asked; the page arrives here as a turn if they tap it"
            },
        }))
    }
}
