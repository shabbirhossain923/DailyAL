use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::Config;
use aws_sdk_dynamodb::types::AttributeValue;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug, Clone)]
pub struct CacheService {
    pub config: Config,
    pub mem_cache: Arc<Mutex<HashMap<String, (String, i64)>>>,
}

impl CacheService {
    pub fn new(config: Config) -> CacheService {
        CacheService {
            config,
            mem_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn cache_insert(&self, key: String, value: String, ttl: Option<i64>) {
        let now = chrono::Utc::now().timestamp();
        let exp = ttl.map(|t| now + t).unwrap_or(i64::MAX);
        if let Ok(mut guard) = self.mem_cache.lock() {
            guard.insert(key, (value, exp));
        }
    }

    fn cache_get(&self, key: &str) -> Option<String> {
        let now = chrono::Utc::now().timestamp();
        if let Ok(mut guard) = self.mem_cache.lock() {
            if let Some((value, exp)) = guard.get(key) {
                if now <= *exp {
                    return Some(value.clone());
                } else {
                    guard.remove(key);
                }
            }
        }
        None
    }

impl CacheService {
    pub async fn check_aws_get_item(&self) -> () {
        match self
            .config
            .secrets
            .dynamo_db
            .get_item()
            .table_name(self.config.secrets.table_name.clone())
            .key("pk", AttributeValue::S("PK#HEALTH".to_string()))
            .key("sk", AttributeValue::S("HEALTH".to_string()))
            .send()
            .await
            .unwrap()
            .item
        {
            Some(item) => {
                println!("Item found in AWS DynamoDB, {:?}", item);
            }
            None => {
                panic!("No item found in AWS DynamoDB");
            }
        }
    }
    pub async fn get_cache_by_id<T: DeserializeOwned + Clone>(
        &self,
        content_type: &str,
        id: String,
    ) -> Option<T> {
        self.get_by_id(content_type, id, "CACHE").await
    }
    pub async fn get_link_by_id<T: DeserializeOwned + Clone>(
        &self,
        content_type: &str,
        id: String,
    ) -> Option<T> {
        self.get_by_id(content_type, id, "LINK").await
    }
    pub async fn get_by_id<T: DeserializeOwned + Clone>(
        &self,
        content_type: &str,
        id: String,
        pk_type: &str,
    ) -> Option<T> {
        let pk = format!("{}#{}_{}", pk_type, content_type, id);
        if let Some(json) = self.cache_get(&pk) {
            if let Ok(value) = serde_json::from_str::<Vec<T>>(&json) {
                if let Some(first) = value.into_iter().next() {
                    return Some(first);
                }
            }
        }
        let client = self.config.secrets.dynamo_db.clone();
        let result = client
            .get_item()
            .table_name(self.config.secrets.table_name.clone())
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S("metadata".to_string()))
            .send()
            .await
            .map(|x| x.item)
            .map(|item| match item {
                Some(item) => deserialize_item(item),
                None => None,
            });
        match result {
            Ok(item) => item,
            Err(_) => None,
        }
    }

    fn add_ttl_in_secs_to_current_time(&self, ttl: i64) -> i64 {
        let current_time = chrono::Utc::now().timestamp();
        return current_time + ttl;
    }

    pub async fn set_cache_by_id<T: Serialize + std::marker::Sync + std::marker::Send>(
        &self,
        content_type: &str,
        id: String,
        data: &T,
        expiry: Option<i64>,
    ) -> Option<()> {
        self.set_by_id("CACHE", format!("{}_{}", content_type, id), data, expiry).await
    }

    pub async fn set_by_id<T: Serialize + std::marker::Sync + std::marker::Send>(
        &self,
        pk_type: &str,
        id: String,
        data: &T,
        expiry: Option<i64>,
    ) -> Option<()> {
        let pk = format!("{}#{}", pk_type, id);
        let vec_data = vec![data];
        let value = serde_json::to_string(&vec_data).unwrap();
        self.cache_insert(pk.clone(), value.clone(), expiry);
        let mut item_builder = self
            .config
            .secrets
            .dynamo_db
            .put_item()
            .table_name(self.config.secrets.table_name.clone())
            .item("pk".to_string(), AttributeValue::S(pk))
            .item("sk".to_string(), AttributeValue::S("metadata".to_string()))
            .item("cached_data".to_string(), AttributeValue::S(value));
        if expiry.is_some() {
            let ttl = self.add_ttl_in_secs_to_current_time(expiry.unwrap());
            item_builder = item_builder.item("ttl".to_string(), AttributeValue::N(ttl.to_string()));
        }
        let result = item_builder.send().await;
        match result {
            Ok(_) => Some(()),
            Err(_) => None,
        }
    }


    pub async fn set_link_by_id(
        &self,
        id: &str,
        data: HashMap<String, String>,
        expiry: Option<i64>,
    ) -> Option<()> {
        let pk = format!("LINK#{}", id);
        let mut item_builder = self
            .config
            .secrets
            .dynamo_db
            .put_item()
            .table_name(self.config.secrets.table_name.clone())
            .item("pk".to_string(), AttributeValue::S(pk))
            .item("sk".to_string(), AttributeValue::S("metadata".to_string()));
        for (key, value) in data {
            item_builder = item_builder.item(key, AttributeValue::S(value));
        }
        if expiry.is_some() {
            let ttl = self.add_ttl_in_secs_to_current_time(expiry.unwrap());
            item_builder = item_builder.item("ttl".to_string(), AttributeValue::N(ttl.to_string()));
        }
        let result = item_builder.send().await;
        match result {
            Ok(_) => Some(()),
            Err(_) => None,
        }
    }

    pub async fn delete_by_id(&self, content_type: &str, id: String) {
        let pk = format!("CACHE#{}_{}", content_type, id);
        let client = self.config.secrets.dynamo_db.clone();
        let result = client
            .delete_item()
            .table_name(self.config.secrets.table_name.clone())
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S("metadata".to_string()))
            .send()
            .await;
        match result {
            Ok(_) => (),
            Err(_) => (),
        }
    }
}

fn deserialize_item<T: DeserializeOwned + Clone>(
    item: std::collections::HashMap<String, AttributeValue>,
) -> Option<T> {
    let json = item
        .get("cached_data")
        .or(item.get("data"))
        .unwrap()
        .as_s()
        .as_ref()
        .unwrap()
        .to_string();
    let vector: Vec<T> = serde_json::from_str(&json).unwrap();
    Some(vector[0].clone())
}
