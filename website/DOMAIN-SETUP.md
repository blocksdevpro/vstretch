# vstretch domain connection

The site is published privately at https://vstretch-blocksdev.mail900512.chatgpt.site.

The custom domain `vstretch.blocksdev.pro` is registered with Sites and pending DNS verification. Add these records at the DNS provider for `blocksdev.pro`.

| Type | Full record name | Value |
| --- | --- | --- |
| CNAME | `vstretch.blocksdev.pro` | `custom-domains.chatgpt.site.` |
| TXT | `_openai-site-verification.vstretch.blocksdev.pro` | `openai-site-verification=Z_1tgY00qkYpl3cO6E6qnluYMjlTO2OnCAApgBXBHb4` |
| TXT | `_cf-custom-hostname.vstretch.blocksdev.pro` | `55df0a62-e23b-4eaf-9ed3-c6f3da48fceb` |

Some DNS providers append `blocksdev.pro` automatically. In that case, use `vstretch`, `_openai-site-verification.vstretch`, and `_cf-custom-hostname.vstretch` as the record names.

Keep the CNAME in DNS-only mode if your provider offers proxying. Wait for domain and certificate verification after adding the records. The site remains private until its audience is changed explicitly.

The landing page uses launch copy for the upcoming tray release. Publish that app release before making the site public, so the latest-download links deliver the tray version.
