use std::io::Write;
use std::path::Path;

use openssl::ssl::{SslContext, SslMethod};
use openssl::ssl::error::SslError;
use openssl::x509::X509FileType;

use hyper::net::{OpensslClient, HttpsConnector, Fresh};
use hyper::method::Method;
use hyper::client::{Client, RequestBuilder};
use hyper::client::request::Request;

use url::Url;

pub fn ssl_context<C>(cacert: C, cert: Option<C>, key: Option<C>) -> Result<OpensslClient, SslError>
    where C: AsRef<Path>
{
    let mut ctx = SslContext::new(SslMethod::Tlsv1_2).unwrap();
    try!(ctx.set_cipher_list("DHE-RSA-AES128-GCM-SHA256:DHE-RSA-AES256-GCM-SHA384:\
                              DHE-RSA-AES128-SHA256:DHE-RSA-AES256-SHA256:\
                              DHE-RSA-CAMELLIA128-SHA:DHE-RSA-AES128-SHA:\
                              DHE-RSA-CAMELLIA256-SHA:DHE-RSA-AES256-SHA:AES128-GCM-SHA256:\
                              AES256-GCM-SHA384:CAMELLIA128-SHA:AES128-SHA:!aNULL:!eNULL:\
                              !EXPORT:!DES:!3DES:!RC4:!MD5"));
    try!(ctx.set_CA_file(cacert.as_ref()));
    // TODO should validate both key and cert are set when either one is
    // specified
    if let Some(cert) = cert {
        try!(ctx.set_certificate_file(cert.as_ref(), X509FileType::PEM));
    };
    if let Some(key) = key {
        try!(ctx.set_private_key_file(key.as_ref(), X509FileType::PEM));
    };
    Ok(OpensslClient::new(ctx))
}

pub fn ssl_connector<C>(cacert: C, cert: Option<C>, key: Option<C>) -> HttpsConnector<OpensslClient>
    where C: AsRef<Path>
{
    let ctx = match ssl_context(cacert, cert, key) {
        Ok(ctx) => ctx,
        Err(e) => pretty_panic!("Error opening certificate files: {}", e),
    };
    HttpsConnector::new(ctx)
}

header! { (XAuthentication, "X-Authentication") => [String] }

pub enum Auth {
    CertAuth {
        cacert: String,
        cert: String,
        key: String,
    },
    NoAuth,
    TokenAuth {
        cacert: String,
        token: String,
    },
}

impl Auth {
    pub fn client(&self) -> Client {
        match self {
            &Auth::CertAuth { ref cacert, ref cert, ref key } => {
                let conn = ssl_connector(Path::new(cacert),
                                         Some(Path::new(cert)),
                                         Some(Path::new(key)));
                Client::with_connector(conn)
            }
            &Auth::TokenAuth { ref cacert, .. } => {
                let conn = ssl_connector(Path::new(cacert), None, None);
                Client::with_connector(conn)
            }
            &Auth::NoAuth => Client::new(),
        }
    }

    pub fn request(&self, method: Method, url: Url) -> Request<Fresh> {
        match self {
            &Auth::CertAuth { ref cacert, ref cert, ref key } => {
                let conn = ssl_connector(Path::new(cacert),
                                         Some(Path::new(cert)),
                                         Some(Path::new(key)));
                Request::<Fresh>::with_connector(method, url, &conn).unwrap()
            }
            &Auth::TokenAuth { ref cacert, ref token, .. } => {
                let conn = ssl_connector(Path::new(cacert), None, None);
                let mut req = Request::<Fresh>::with_connector(method, url, &conn).unwrap();
                req.headers_mut().set(XAuthentication(token.clone()));
                req
            }
            &Auth::NoAuth => Request::<Fresh>::new(method, url).unwrap(),
        }
    }

    pub fn auth_header<'a>(&self, request_builder: RequestBuilder<'a>) -> RequestBuilder<'a> {
        match self {
            &Auth::TokenAuth { ref token, .. } => {
                request_builder.header(XAuthentication(token.clone()))
            }
            _ => request_builder,
        }
    }
}

/// Checks whether the vector of urls contains a url that needs to use SSL, i.e.
/// has `https` as the scheme.
pub fn is_ssl(server_urls: &Vec<String>) -> bool {
    server_urls.into_iter()
        .any(|url| {
            "https" ==
                Url::parse(&url)
                .unwrap_or_else(|e| pretty_panic!("Error parsing url {:?}: {}", url, e))
                .scheme()
        })
}
