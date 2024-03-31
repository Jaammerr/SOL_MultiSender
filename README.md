# Solana MultiSender

## 🔗 Links

🔔 CHANNEL: https://t.me/JamBitPY

💬 CHAT: https://t.me/JamBitChat

💰 DONATION EVM ADDRESS: 0x08e3fdbb830ee591c0533C5E58f937D312b07198


## 📝 Functions 

1. Withdraw entered amount SOL from main to each subaccount
2. Withdraw all SOL from subaccounts to main

``Once launched you will be able to select one of the functions``



## ⚙️ Configuration (.env)

`` HTTP_RPC_URL - custom RPC url (if not, leave it as default)``

`` MAIN_WALLET - private key of the wallet from which Solana will be sent to subaccounts``

`` THREADS - number of threads (no more than 10 recommended)``


## ⚙️ Sub accounts (files > accounts.txt)
    
    Each line is a new account
    
    The format is: private key
    
    Example: 
    key1
    key2
    key3

## 🚀 Installation
`Download and install rust: https://www.rust-lang.org/tools/install`

```bash 
Clone the repo: git clone https://github.com/Jaammerr/SOL_MultiSender.git
Open CMD (console) in the folder
Run: cargo run
```

