use keyring::Entry;

const TOKEN_SERVICE: &str = "NovaCibesEditor";
const TOKEN_USER: &str = "HFApiToken";

pub struct TokenManager;

impl TokenManager {
    fn keyring_entry() -> Option<Entry> {
        Entry::new(TOKEN_SERVICE, TOKEN_USER).ok()
    }

    pub fn load_token() -> Option<String> {
        if let Some(entry) = Self::keyring_entry() {
            match entry.get_password() {
                Ok(token) => {
                    if !token.is_empty() {
                        println!("Token loaded from keyring successfully.");
                        Some(token)
                    } else {
                        println!("Token found in keyring but was empty.");
                        None
                    }
                }
                Err(e) => {
                    eprintln!("Error reading token from keyring: {:?}", e);
                    None
                }
            }
        } else {
            eprintln!("Failed to initialize keyring entry for loading.");
            None
        }
    }

    pub fn save_token(token: &str) {
        if let Some(entry) = Self::keyring_entry() {
            if let Err(e) = entry.set_password(token) {
                eprintln!("Error saving token to keyring: {:?}", e);
            } else {
                 println!("Token saved to keyring successfully.");
            }
        } else {
            eprintln!("Failed to initialize keyring entry for saving.");
        }
    }

    // Note: delete_credential exists in keyring 2.x
    pub fn remove_token() {
         if let Some(entry) = Self::keyring_entry() {
             if let Err(e) = entry.delete_credential() {
                 eprintln!("Error deleting token from keyring: {:?}", e);
             } else {
                  println!("Token removed from keyring successfully.");
             }
         } else {
             eprintln!("Failed to initialize keyring entry for removal.");
         }
    }
}