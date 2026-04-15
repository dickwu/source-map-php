<?php

use Hyperf\HttpServer\Router\Router;
use App\Controller\ConsentController;

Router::addRoute('POST', '/consents', [ConsentController::class, 'store']);
