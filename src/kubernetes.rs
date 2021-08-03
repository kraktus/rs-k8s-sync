use isahc::{HttpClient, config::CaCertificate, config::ClientCertificate, config::PrivateKey, config::Configurable, config::SslOption, Request};
use std::{io::Read, io::Write};
use k8s_openapi::api::core::v1 as api;
use crate::errors::KubernetesError;
use crate::config::KubeConfig;
use base64;
use tempfile::NamedTempFile;

#[derive(Debug)]
pub struct Kubernetes {
    token: Option<String>,
    pub kubeconfig: Result<KubeConfig, KubernetesError>,
    pub http_client: HttpClient,
}

impl Kubernetes {
    pub fn connect(kubeconfig_path: Option<String>) -> Result<Kubernetes, KubernetesError> {
        let token = None;
        let kubeconfig = KubeConfig::load(kubeconfig_path);
        let http_client;
        if let Ok(conf) = &kubeconfig {
            //TODO add options, guessed from config
            if let Some(cluster) = conf.clusters.first() {
                if let Some(auth_info) = conf.auth_infos.first() {
                    let user = &auth_info.auth_info;
                    if let Some(crt) = &user.client_certificate_data {
                        if let Some(ca) = &cluster.cluster.certificate_authority_data {
                            if let Some(key) = &user.client_key_data {
                                let mut tmpfile = NamedTempFile::new().map_err(|err| KubernetesError::IoError { source: err })?;
                                writeln!(tmpfile, "{}", ca).map_err(|err| KubernetesError::IoError { source: err })?;
                                let http_client_builder = HttpClient::builder()
                                    .ssl_client_certificate(
                                        ClientCertificate::pem(
                                                base64::decode(crt).map_err(|err| KubernetesError::Base64DecodeError { source: err })?,
                                                PrivateKey::pem(base64::decode(key).map_err(|err| KubernetesError::Base64DecodeError { source: err })?, None)
                                            )
                                    ).ssl_ca_certificate(                                
                                        CaCertificate::file(
                                            tmpfile.into_temp_path().to_path_buf()
                                        )
                                    )   
                                    .ssl_options(SslOption::DANGER_ACCEPT_INVALID_CERTS);                            
                                http_client = match http_client_builder.build() {
                                    Ok(client) => client,
                                    Err(err) => return Err(KubernetesError::HttpClientBuildError { message: format!("Failed to initialize http client with client certificate: {}", err) })
                                };  
                            } else {
                                return Err(KubernetesError::HttpClientBuildError { message: String::from("Couldn't get client key from kubeconfig.")})
                            }
                        } else {
                            return Err(KubernetesError::HttpClientBuildError { message: String::from("Couldn't get CA certificate from kubeconfig.")})
                        }
                    } else {
                        return Err(KubernetesError::HttpClientBuildError { message: String::from("Couldn't get client private key.")})
                    }
                } else {
                    return Err(KubernetesError::HttpClientBuildError { message: String::from("No auth_info item found in kubeconfig.") })
                }
            } else {
                return Err(KubernetesError::ConfigLoadError)
            }
        } else {
            return Err(KubernetesError::HttpClientBuildError { message: String::from("Couldn't gather kubeconfig content.") })
        }

        Ok(
            Kubernetes {
                token,
                kubeconfig,
                http_client
            }
        )
    }

    //fn list_pods(&self, namespace: String) -> std::io::Result<Vec<api::Pod>>{
    pub fn list_pods(&self, namespace: String) -> Result<Vec<String>, KubernetesError>{
        let (request, response_body) = match api::Pod::list_namespaced_pod(&namespace, Default::default()) {
            Ok((request, response_body)) => (request, response_body),
            Err(err) => return Err(KubernetesError::ApiRequestError { source: err }),
        };
        let (parts, body) = request.into_parts();
        let uri_str = format!("https://localhost:6443{}", parts.uri);
        let request = Request::builder()
            .uri(uri_str).body(body).map_err(|err| KubernetesError::HttpClientBuildError { message: String::from("Couldn't build request.") })?;
        let mut response = self.http_client.send(request).map_err(|err| KubernetesError::HttpClientRequestError)?;
        println!("Got the response: {:?}", response);
        let status_code = response.status(); 
        if !status_code.is_success(){
            return Err(KubernetesError::HttpClientRequestError);
        }
        //let mut response_body = response_body(status_code);
        let mut buf = String::new();
        let mut pods_list = vec![];
        let read = response.body_mut().read_to_string(&mut buf);
        match read {
            Ok(res) => {
                pods_list.push(buf.clone());
            },
            Err(err) => eprintln!("ERR got {}", err),
        }
        //for pod in &pods_list {
        //    println!("{:#?}", pod);
        //}

        Ok(pods_list)
    }
}
